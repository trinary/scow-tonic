use std::collections::HashMap;

use std::sync::Arc;

use tokio::time::Instant;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

use crate::db::Db;
use crate::{scow::*, Peer};
use crate::scow_key_value_server::ScowKeyValue;

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Follower, 
    Leader, 
    Candidate
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
            leader_state: None
        }
    }
}

pub struct LeaderState {
    /// for each server, index of the next log entry to send to that server (initialized to leaderlast log index + 1)
    next_index: HashMap<u64, u64>,
    /// for each server, index of highest log entry known to be replicated on server (initialized to 0, increases monotonically)
    match_index: HashMap<u64, u64>,
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
            Some(s) => {
                Ok(Response::new(GetReply {
                    value: s
                }))
            },
            None => Err(Status::not_found("key not found")),
        }        
    }

    async fn set(&self, request: Request<SetRequest>) -> Result<Response<SetReply>, Status> {
        let inner = request.into_inner();
        let _db_response = self.db.set(&inner.key, &inner.value);
        Ok(Response::new(SetReply { success: true}))
    }

    async fn append_entries(&self, request: Request<AppendEntriesRequest>) -> Result<Response<AppendEntriesReply>, Status> {
        let inner = request.into_inner();
        for i in inner.entries {
            self.db.set(&i.key, &i.value);
        }

        tracing::debug!("asking for server_state in append_entries");
        //let mut state_inner = self.server_state.lock().unwrap();
        //state_inner.last_heartbeat = Instant::now();
        tracing::debug!("DONE with server_state in append_entries");
        Ok(Response::new(AppendEntriesReply { term: 0, success: true}))
    }

    async fn request_vote(&self, request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteReply>, Status> {
        let inner = request.into_inner();
        let server_result = self.server_request_vote(
            inner.term, 
            inner.candidate_id, 
            inner.last_log_index
        ).await;
        Ok(Response::new(RequestVoteReply { term: server_result.0, vote_granted: server_result.1 }))
    }
}

impl MyScowKeyValue<> {
    pub fn new(server_state: Arc<Mutex<ServerState>>) -> Self {
        MyScowKeyValue {
            db: Db::new(),
            peers: vec![],
            server_state: server_state,
        }
    }

    pub async fn server_request_vote(&self, candidate_term: u64, candidate_id: u64, candidate_last_index: u64) -> (u64, bool) {
        tracing::debug!("about to ask for state mutex.");
        // TODO: we need to get the ServerState here somehow. Only have it be owned by one thing! Right now
        // that one thing is ElectionHandler, but that seems wrong. ServerStateManager should provide ServerState
        // to both ElectionHandler (and soon HeartbeatHandler or whatever else) as well as the KeyValue piece
        // KV (tonic service) needs it in order to respond to vote requests and AppendEntries, and Handler types
        // need it to do their server maintenance stuff. But it should only "live" in one place!!!

        let mut server_state = self.server_state.lock().await;
        
        tracing::debug!("got state mutex.");
        if candidate_term < server_state.current_term {
            server_state.voted_for = None;
            (server_state.current_term, false)
        } else {
            if (server_state.voted_for == None || server_state.voted_for == Some(candidate_id)) &&
            candidate_last_index >= server_state.last_log_index {
                server_state.voted_for = Some(candidate_id);
                (server_state.current_term, true)
            } else {
                server_state.voted_for = None;
                (server_state.current_term, false)
            }
        }
    }
}
