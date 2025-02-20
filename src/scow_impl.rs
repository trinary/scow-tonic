use std::error::Error;

use prost::bytes::Bytes;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::time::Instant;
use tonic::{Request, Response, Status};

use crate::db::Db;
use crate::scow::*;
use crate::scow_key_value_server::ScowKeyValue;
use crate::state_handler::{StateCommand, StateCommandResult};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Follower,
    Leader,
    Candidate,
}

#[derive(Clone, Copy, Debug)]
pub struct ServerState {
    pub id: u64,
    pub role: Role,
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub last_heartbeat: Instant,
    pub last_log_index: u64,
    pub leader_state: Option<LeaderState>,
}

impl ServerState {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            role: Role::Follower,
            current_term: 0,
            voted_for: None,
            last_heartbeat: Instant::now(), // this could cause problems, it should be some min value like epoch.
            last_log_index: 0,
            leader_state: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LeaderState {
    /// for each server, index of the next log entry to send to that server (initialized to leaderlast log index + 1)
    next_index: u64,
    /// for each server, index of highest log entry known to be replicated on server (initialized to 0, increases monotonically)
    match_index: u64,
}
impl LeaderState {
    pub(crate) fn new() -> LeaderState {
        Self {
            next_index: 0,
            match_index: 0,
        }
    }
}

pub struct MyScowKeyValue {
    db: Db,
    command_handler_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>,
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

        // we need to reject writes if we aren't the leader.
        tracing::info!("asking for server_state in set");
        let (response_tx, response_rx) = oneshot::channel();
        self.command_handler_tx
            .send((StateCommand::GetServerState, response_tx))
            .await
            .map_err(|e| Status::from_error(Box::new(e)))?; // TODO: error handling
        let state_result = response_rx.await.unwrap();
        let state = match state_result {
            StateCommandResult::StateResponse(state) => state,
            _s => panic!("got the wrong result from a read op to state handler"),
        };

        if state.role != Role::Leader {
            Err(Status::with_details(tonic::Code::FailedPrecondition, "This server is not the leader.", Bytes::new()))
        } else {

            // TODO we need to propogate writes to other clients via appendEntries when this write occurs
            let _db_response = self.db.set(&inner.key, &inner.value);

            let (cmd_tx, cmd_rx) = oneshot::channel();
            self.command_handler_tx
                .send((StateCommand::DistributeWrites(vec![inner]), cmd_tx))
                .await
                .map_err(|e| Status::from_error(Box::new(e)))?;
            let cmd_result= cmd_rx.await.unwrap();
            match cmd_result {
                _ => Ok(Response::new(SetReply { success: true })),

                // StateCommandResult::StateResponse(server_state) => todo!(),
                
                // StateCommandResult::HeartbeatResponse(vec) => todo!(),
                // StateCommandResult::RequestVoteResponse(vec) => todo!(),
                // StateCommandResult::StateSuccess => todo!(),
            }
        } 
        
    }

    async fn append_entries(
        &self,
        request: Request<AppendEntriesRequest>,
    ) -> Result<Response<AppendEntriesReply>, Status> {
        let inner = request.into_inner();

        tracing::info!("asking for server_state in append_entries");
        let (response_tx, response_rx) = oneshot::channel();
        self.command_handler_tx
            .send((StateCommand::GetServerState, response_tx))
            .await
            .map_err(|e| Status::from_error(Box::new(e)))?; // TODO: error handling
        let state_result = response_rx.await.unwrap();
        let mut state = match state_result {
            StateCommandResult::StateResponse(state) => state,
            _s => panic!("got the wrong result from a read op to state handler"),
        };

        state.last_heartbeat = Instant::now();
        // we got a heartbeat from someone, so someone is a leader and we should become a follower.
        // and set current term to theirs.
        // that is, if their term is greater than ours

        let response = if inner.leader_term >= state.current_term {
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
        };

        for i in inner.entries {
            tracing::info!("WROTE A KEY FROM APPEND 🔑🔑🔑🔑🔑🔑");
            self.db.set(&i.key, &i.value);
        }

        let (state_update_tx, state_update_rx) = oneshot::channel();
        self.command_handler_tx
            .send((StateCommand::SetServerState(state), state_update_tx))
            .await
            .map_err(|e| Status::from_error(Box::new(e)))?; // TODO error handling

        match state_update_rx.await {
            Ok(update) => tracing::info!(
                "got a result from updating state after handling append_entries: {:?}",
                update
            ),
            Err(e) => {
                tracing::error!(
                    "got an ERR from updating state after handling append_entries: {:?}",
                    e
                )
            }
        };
        response
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
    pub fn new(
        server_state_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>,
    ) -> Self {
        MyScowKeyValue {
            db: Db::new(),
            command_handler_tx: server_state_tx,
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
        self.command_handler_tx
            .send((StateCommand::GetServerState, response_tx))
            .await?; // TODO: error handling
        let state_result = response_rx.await.unwrap();
        let mut state = match state_result {
            StateCommandResult::StateResponse(state) => state,
            _ => panic!("got the wrong result from a read op to state handler"),
        };

        let mut vote = false;

        tracing::info!("IMPL got a ServerState from the handler: {:?}", state);

        if candidate_term <= state.current_term {
            state.voted_for = None;
        } else if (state.voted_for.is_none() || state.voted_for == Some(candidate_id))
            && candidate_last_index >= state.last_log_index
        {
            state.voted_for = Some(candidate_id);
            vote = true;
        } else {
            state.voted_for = None;
        }

        let (write_response_tx, write_response_rx) = oneshot::channel();

        self.command_handler_tx
            .send((StateCommand::SetServerState(state), write_response_tx))
            .await?;

        let vote_response = write_response_rx.await;
        match vote_response {
            Ok(r) => {
                tracing::info!(
                    "got a result from our state write handling vote request {:?}",
                    r
                );
            }
            Err(e) => {
                tracing::error!("ERROR: from our state write handling vote request {:?}", e);
            }
        }

        Ok((state.current_term, vote))
    }
}
