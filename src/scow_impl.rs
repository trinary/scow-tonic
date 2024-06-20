use std::collections::HashMap;

use std::error::Error;

use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::time::Instant;
use tonic::{Request, Response, Status};

use crate::db::Db;
use crate::scow_key_value_server::ScowKeyValue;
use crate::state_handler::{StateCommand, StateCommandResult};
use crate::scow::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Follower,
    Leader,
    Candidate,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
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
    server_state_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>,
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
        let (response_tx, response_rx) = oneshot::channel();
        self.server_state_tx.send((StateCommand::GetServerState, response_tx)).await.ok().unwrap(); // TODO: bomb
        let state_result = response_rx.await.unwrap();
        let mut state = match state_result {
            StateCommandResult::StateResponse(state) => state,
            _s => panic!("got the wrong result from a read op to state handler") 
        };

        state.last_heartbeat = Instant::now();
        // we got a heartbeat from someone, so someone is a leader and we should become a follower.
        // and set current term to theirs.
        // that is, if their term is greater than ours

        if inner.leader_term >= state.current_term {
            // shouldnt be possible to have an equal term, but just in case?
            state.role = Role::Follower; // does this fuck up if we're a candidate when this happens?
            state.current_term = inner.leader_term;
            Ok(Response::new(AppendEntriesReply {
                term: state.current_term,
                success: true,
            }))
        } else {
            // the leader is further behind in terms than we are!
            Ok(Response::new(AppendEntriesReply {
                term: state.current_term,
                success: false,
            }))
        }
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
    pub fn new(server_state_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>) -> Self {
        MyScowKeyValue {
            db: Db::new(),
            server_state_tx: server_state_tx,
        }
    }

    pub async fn server_request_vote(
        &self,
        candidate_term: u64,
        candidate_id: u64,
        candidate_last_index: u64,
    ) -> Result<(u64, bool), Box<dyn Error>> {
        tracing::info!("about to ask for state mutex in server_request_vote");

        tracing::info!("asking for server_state in append_entries");
        let (response_tx, response_rx) = oneshot::channel();
        self.server_state_tx.send((StateCommand::GetServerState, response_tx)).await.ok().unwrap(); // TODO: bomb
        let state_result = response_rx.await.unwrap();
        let mut state = match state_result {
            StateCommandResult::StateResponse(state) => state,
            _ => panic!("got the wrong result from a read op to state handler") 
        };

        let mut vote = false;

        tracing::info!("IMPL got a ServerState from the handler: {:?}", state);

        if candidate_term <= state.current_term {
            state.voted_for = None;
        } else {
            if (state.voted_for.is_none() || state.voted_for == Some(candidate_id))
                && candidate_last_index >= state.last_log_index
            {
                state.voted_for = Some(candidate_id);
                vote = true;
            } else {
                state.voted_for = None;
            }
        }
        let (write_response_tx, write_response_rx) = oneshot::channel();
        let write_result = self.server_state_tx.send((StateCommand::SetServerState(state.clone()), write_response_tx)).await?;
        tracing::info!("got a result from our state write in request");
        Ok((state.current_term, vote))
    }
}
