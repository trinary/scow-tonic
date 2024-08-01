use std::{fmt::Debug, sync::Arc};

use tokio::{
    sync::{mpsc::Receiver, oneshot, Mutex},
    task::JoinSet,
};
use tonic::{transport::Channel, Status};

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
}

#[derive(Debug)]
pub enum StateCommandResult {
    StateResponse(ServerState),
    HeartbeatResponse(Vec<Result<tonic::Response<scow::AppendEntriesReply>, Status>>),
    RequestVoteResponse(Vec<Result<tonic::Response<scow::RequestVoteReply>, Status>>), // todo these shouldnt be bound to our specific server/client types
    StateSuccess,
}

// #[derive(Debug)]
// pub enum StateCommandError {
//     BasicError(String)
// }

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
            .flat_map(|p| client_tools::build_client(p))
            .collect();

        tracing::info!("built clients in StateHandler new: {:?}", clients);
        Self {
            rx,
            server_state: Arc::new(Mutex::new(ServerState::new(my_id))),
            clients: clients,
        }
    }

    pub async fn run(&mut self) -> () {
        tracing::info!("StateHandler has started.");
        while let Some((cmd, response_channel)) = self.rx.recv().await {
            let r = self.handle_command(cmd, response_channel).await;
            tracing::info!("handled a cmd, got a {:?}", r);
        }
    }

    async fn set_server_state(&mut self, new_state: ServerState) -> () {
        tracing::info!("StateHandler got a SetServerState command. 📝");
        let mut current_state = self.server_state.lock().await;
        *current_state = new_state;
        tracing::info!("StateHandler set the state. 📝");
    }

    async fn heartbeat(
        &mut self,
        server_state: ServerState,
    ) -> Vec<Result<tonic::Response<scow::AppendEntriesReply>, Status>> {
        // TODO: figure out how to make many parallel requests to these clients without blocking.
        // we already had this problem in heartbeat.rs, we may need our own sub-channel to parallelize
        // LETS TRY

        let mut joinset = JoinSet::new();
        let mut results = vec![];

        for mut client in
            <Vec<ScowKeyValueClient<Channel>> as Clone>::clone(&self.clients).into_iter()
        {
            joinset.spawn(async move {
                tracing::info!("sending append_entries HEARTBEAT to {:?}", client);
                let append_result = client
                    .append_entries(AppendEntriesRequest {
                        leader_term: server_state.current_term,
                        leader_id: server_state.id,
                        prev_log_index: 33333,
                        prev_log_term: 44444,
                        leader_commit: 55555,
                        entries: vec![],
                    })
                    .await;
                tracing::info!("append_entries HEARTBEAT COMPLETE");
                append_result
            });
        }

        while let Some(res) = joinset.join_next().await {
            if res.is_ok() {
                results.push(res.unwrap());
            }
        }

        tracing::info!("done collecting replies from heartbeat.");

        results
    }

    async fn handle_command(
        &mut self,
        cmd: StateCommand,
        response_channel: oneshot::Sender<StateCommandResult>,
    ) -> Result<(), String> {
        match cmd {
            StateCommand::GetServerState => {
                tracing::info!("StateHandler got a GetServerState command. 📖");
                let state = self.server_state.lock().await.clone();
                match response_channel.send(StateCommandResult::StateResponse(state)) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        tracing::error!(
                            "ERROR sending response from GetServerState command: {:?}",
                            e
                        );
                        Err(String::from("GetServerState"))
                    }
                }
            }
            StateCommand::SetServerState(s) => {
                self.set_server_state(s).await;
                match response_channel.send(StateCommandResult::StateSuccess) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        tracing::error!(
                            "ERROR sending response from SetServerState command: {:?}",
                            e
                        );
                        Err(String::from("SetServerState"))
                    }
                }
            }
            StateCommand::Heartbeat => {
                let state = self.server_state.lock().await.clone();
                let results = self.heartbeat(state).await;
                match response_channel.send(StateCommandResult::HeartbeatResponse(results)) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        tracing::error!(
                            "ERROR sending response from Heartbeat command: {:?}",
                            e
                        );
                        Err(String::from("Heartbeat"))
                    }
                }
            }
            StateCommand::RequestVote => {
                let state = self.server_state.lock().await.clone();
                let results = self.request_vote(state).await;
                match response_channel.send(StateCommandResult::RequestVoteResponse(results)) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        tracing::error!(
                            "ERROR sending response from RequestVote command: {:?}",
                            e
                        );
                        Err(String::from("RequestVote"))
                    }
                }
            }
        }
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
            if res.is_ok() {
                results.push(res.unwrap());
            }
        }
        results
    }
}
