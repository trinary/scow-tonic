use std::collections::HashMap;

use std::error::Error;
use std::hash::Hash;
use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::time::Instant;
use tonic::{Request, Response, Status};

use crate::db::Db;
use crate::scow_key_value_server::ScowKeyValue;
use crate::{scow::*, Peer};

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Follower,
    Leader,
    Candidate,
}

pub struct ServerState {
    pub role: Role,
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub last_heartbeat: Instant,
    pub last_log_index: u64,
    pub leader_state: Option<LeaderState>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            role: Role::Follower,
            current_term: 0,
            voted_for: None,
            last_heartbeat: Instant::now(), // this could cause problems, it should be some min value like epoch.
            last_log_index: 0,
            leader_state: None,
        }
    }
}

pub struct LeaderState {
    /// for each server, index of the next log entry to send to that server (initialized to leaderlast log index + 1)
    next_index: HashMap<u64, u64>,
    /// for each server, index of highest log entry known to be replicated on server (initialized to 0, increases monotonically)
    match_index: HashMap<u64, u64>,
}
impl LeaderState {
    pub(crate) fn new(leader_last_log: u64) -> LeaderState {
        let mut this = Self {
            next_index: HashMap::new(),
            match_index: HashMap::new(),
        };

        this.next_index.insert(1, leader_last_log);
        this.match_index.insert(1, 0);
        this
    }
}

pub struct MyScowKeyValue {
    db: Db,
    peers: Vec<Peer>,
    // TODO: so here we are. lifetime specifier required for this.
    server_state: Arc<Mutex<ServerState>>,
}

#[tonic::async_trait]
impl ScowKeyValue for MyScowKeyValue {
    async fn status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusReply>, Status> {
        let reply = StatusReply {
            status: "Status: OK".to_string(),
        };
        Ok(Response::new(reply))
    }

    async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetReply>, Status> {
        let db_response = self.db.get(request.into_inner().key.as_str());

        match db_response {
            Some(s) => Ok(Response::new(GetReply { value: s })),
            None => Err(Status::not_found("key not found")),
        }
    }

    async fn set(&self, request: Request<SetRequest>) -> Result<Response<SetReply>, Status> {
        let inner = request.into_inner();
        let _db_response = self.db.set(&inner.key, &inner.value);
        Ok(Response::new(SetReply { success: true }))
    }

    async fn append_entries(
        &self,
        request: Request<AppendEntriesRequest>,
    ) -> Result<Response<AppendEntriesReply>, Status> {
        let inner = request.into_inner();
        for i in inner.entries {
            self.db.set(&i.key, &i.value);
        }

        tracing::info!("asking for server_state in append_entries");
        let mut state_result = self.server_state.lock().await;

        // match state_result {
        //     Ok(mut s) => {
        //         s.last_heartbeat = Instant::now();
        //     },
        //     Err(_) => todo!(),
        // }

        state_result.last_heartbeat = Instant::now();
        // we got a heartbeat from someone, so someone is a leader and we should become a follower.
        // and set current term to theirs.
        state_result.role = Role::Follower;
        state_result.current_term = inner.leader_term;
        Ok(Response::new(AppendEntriesReply {
            term: state_result.current_term,
            success: true,
        }))
    }

    async fn request_vote(
        &self,
        request: Request<RequestVoteRequest>,
    ) -> Result<Response<RequestVoteReply>, Status> {
        let inner = request.into_inner();
        let server_result = self
            .server_request_vote(inner.term, inner.candidate_id, inner.last_log_index)
            .await;

        match server_result {
            Ok(res) => Ok(Response::new(RequestVoteReply {
                term: res.0,
                vote_granted: res.1,
            })),
            Err(e) => {
                tracing::error!("Failure result from server_request_vote: {:?}", e);
                Err(Status::unknown("Failure result from server_request_vote"))
            }
        }
    }
}

impl MyScowKeyValue {
    pub fn new(server_state: Arc<Mutex<ServerState>>) -> Self {
        MyScowKeyValue {
            db: Db::new(),
            peers: vec![],
            server_state: server_state,
        }
    }

    pub async fn server_request_vote(
        &self,
        candidate_term: u64,
        candidate_id: u64,
        candidate_last_index: u64,
    ) -> Result<(u64, bool), Box<dyn Error>> {
        tracing::info!("about to ask for state mutex in server_request_vote");

        let mut server_state = self.server_state.lock().await; // only one try_lock? idk
                                                               // if try_lock fails here, that means we're already requesting votes from other servers.
                                                               // Maybe the thing to do here is successfully NOT grant a vote if we know we're requesting? Does that make any sense?
        if candidate_term < server_state.current_term {
            server_state.voted_for = None;
            Ok((server_state.current_term, false))
        } else {
            if (server_state.voted_for.is_none() || server_state.voted_for == Some(candidate_id))
                && candidate_last_index >= server_state.last_log_index
            {
                server_state.voted_for = Some(candidate_id);
                Ok((server_state.current_term, true))
            } else {
                server_state.voted_for = None;
                Ok((server_state.current_term, false))
            }
        }
    }
}
