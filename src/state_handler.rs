use std::{fmt::Debug, sync::Arc};

use tokio::{
    sync::{mpsc::Receiver, oneshot, Mutex},
    task::JoinSet,
};
use tonic::{transport::Channel, Status};

use thiserror::Error;

use crate::{
    scow, scow_impl::ServerState, scow_key_value_client::ScowKeyValueClient, AppendEntriesRequest,
    Peer, RequestVoteRequest,
};

#[path = "./client_tools.rs"]
mod client_tools;

#[derive(Debug)]
pub enum StateCommand {
    GetServerState,
    SetServerState(ServerState),
    Heartbeat,
    RequestVote,
    DistributeWrites(Vec<scow::SetRequest>),
}

#[derive(Debug, Error)]
pub enum StateCommandResult {
    #[error("StateResponse")]
    StateResponse(ServerState),
    #[error("HeartbeatResponse")]
    HeartbeatResponse(Vec<Result<tonic::Response<scow::AppendEntriesReply>, Status>>),
    #[error("RequestVoteResponse")]
    RequestVoteResponse(Vec<Result<tonic::Response<scow::RequestVoteReply>, Status>>), // todo these shouldnt be bound to our specific server/client types
    #[error("StateSuccess")]
    StateSuccess,
}

#[derive(Error, Debug)]
pub enum StateCommandError {
    #[error("idk")]
    StateResponseSendError(#[from] StateCommandResult),
}

pub struct StateHandler {
    rx: Receiver<(StateCommand, oneshot::Sender<StateCommandResult>)>,
    server_state: Arc<Mutex<ServerState>>,
    clients: Vec<ScowKeyValueClient<Channel>>,
}

impl StateHandler {
    pub fn new(
        rx: Receiver<(StateCommand, oneshot::Sender<StateCommandResult>)>,
        peers: &[Peer],
        my_id: u64,
    ) -> Self {
        let clients: Vec<ScowKeyValueClient<Channel>> = peers
            .iter()
            .filter(|s| s.id != my_id)
            .flat_map(client_tools::build_client)
            .collect();

        tracing::info!("built clients in StateHandler new: {:?}", clients);
        Self {
            rx,
            server_state: Arc::new(Mutex::new(ServerState::new(my_id))),
            clients,
        }
    }

    pub async fn run(&mut self) {
        tracing::info!("StateHandler has started.");
        while let Some((cmd, response_channel)) = self.rx.recv().await {
            let r = self.handle_command(cmd, response_channel).await;
            tracing::info!("handled a cmd, got a {:?}", r);
        }
    }

    async fn handle_command(
        &mut self,
        cmd: StateCommand,
        response_channel: oneshot::Sender<StateCommandResult>,
    ) -> Result<(), StateCommandError> {
        match cmd {
            StateCommand::GetServerState => {
                tracing::info!("StateHandler got a GetServerState command. 📖");
                let state = *self.server_state.lock().await;
                // .send returns its argument if if could not be sent, in the Err variant of a Result.
                // that is really confusing behavior! Use Err for an error with helpful error stuff and
                // let the caller do something with the arg if they want to! Weird!

                // this means that our responses also need to be Error to work with other Result-y things like thiserror.
                // This interface might be really terrible, or I am an idiot that doesn't understand the purpose.
                response_channel.send(StateCommandResult::StateResponse(state))?;
                Ok(())
            }
            StateCommand::SetServerState(s) => {
                self.set_server_state(s).await;
                response_channel.send(StateCommandResult::StateSuccess)?;
                Ok(())
            }
            StateCommand::Heartbeat => {
                let state = *self.server_state.lock().await;
                let empty_writes: Arc<Vec<scow::SetRequest>> = Arc::new(vec![]);
                let results = self.heartbeat(state, empty_writes).await;
                response_channel.send(StateCommandResult::HeartbeatResponse(results))?;
                Ok(())
            }
            StateCommand::RequestVote => {
                let state = *self.server_state.lock().await;
                let results = self.request_vote(state).await;
                response_channel.send(StateCommandResult::RequestVoteResponse(results))?;
                Ok(())
            }
            StateCommand::DistributeWrites(writes) => {
                let state = *self.server_state.lock().await;
                let writes_arc = Arc::new(writes);
                let results = self.heartbeat(state, writes_arc.clone()).await;
                response_channel.send(StateCommandResult::HeartbeatResponse(results))?;
                Ok(())
            }
        }
    }

    async fn set_server_state(&mut self, new_state: ServerState) {
        tracing::info!("StateHandler got a SetServerState command. 📝");
        let mut current_state = self.server_state.lock().await;
        *current_state = new_state;
        tracing::info!("StateHandler set the state. 📝");
    }

    async fn heartbeat(
        &self,
        server_state: ServerState,
        writes: Arc<Vec<scow::SetRequest>>,
    ) -> Vec<Result<tonic::Response<scow::AppendEntriesReply>, Status>> {
        let mut joinset = JoinSet::new();
        let mut results = vec![];

        for mut client in
            <Vec<ScowKeyValueClient<Channel>> as Clone>::clone(&self.clients).into_iter()
        {
            let locallll = writes.clone();
            joinset.spawn(async move {
                tracing::info!("sending append_entries HEARTBEAT to {:?}", client);

                let append_result = client
                    .append_entries(AppendEntriesRequest {
                        leader_term: server_state.current_term,
                        leader_id: server_state.id,
                        prev_log_index: 33333,
                        prev_log_term: 44444,
                        leader_commit: 55555,
                        entries:  locallll.to_vec(),
                    })
                    .await;
                tracing::info!("append_entries HEARTBEAT COMPLETE");
                append_result
            });
        }

        while let Some(res) = joinset.join_next().await {
            if let Ok(r) = res {
                results.push(r);
            }
        }

        tracing::info!("done collecting replies from heartbeat.");
        results
    }

    async fn request_vote(
        &self,
        server_state: ServerState,
    ) -> Vec<Result<tonic::Response<scow::RequestVoteReply>, Status>> {
        let mut joinset = JoinSet::new();
        let mut results = vec![];

        for mut client in
            <Vec<ScowKeyValueClient<Channel>> as Clone>::clone(&self.clients).into_iter()
        {
            joinset.spawn(async move {
                tracing::info!("sending a VOTE REQUEST");
                let vote_result = client
                    .request_vote(RequestVoteRequest {
                        term: server_state.current_term,
                        candidate_id: server_state.id,
                        last_log_index: 9999,
                        last_log_term: 8888,
                    })
                    .await;
                tracing::info!("got a response from VOTE REQUEST: {:?}", vote_result);
                vote_result
            });
        }

        while let Some(res) = joinset.join_next().await {
            if let Ok(r) = res {
                results.push(r);
            }
        }
        results
    }
}
