use std::{fmt::Debug, sync::Arc};

use tokio::{sync::{mpsc::Receiver, oneshot, Mutex}, task::{JoinError, JoinSet}};
use tonic::transport::Channel;

use crate::{heartbeat::Heartbeat, scow_impl::ServerState, scow_key_value_client::ScowKeyValueClient, AppendEntriesReply, AppendEntriesRequest, Peer};


#[path = "./client_tools.rs"]
mod client_tools;


#[derive(Debug)]
pub enum StateCommand {
    GetServerState,
    SetServerState(ServerState),
    Heartbeat,
}

#[derive(Debug)]
pub enum StateCommandResult {
    StateResponse(ServerState),
    HeartbeatResponse(Vec<Result<AppendEntriesReply, JoinError>>),
    StateSuccess,
}

// #[derive(Debug)]
// pub enum StateCommandError {
//     BasicError(String)
// }

pub struct StateHandler {
    rx: Receiver<(StateCommand, oneshot::Sender<StateCommandResult>)>,
    server_state: Arc<Mutex<ServerState>>,
    clients: Vec<ScowKeyValueClient<Channel>>
}

impl StateHandler {
    pub fn new(rx: Receiver<(StateCommand, oneshot::Sender<StateCommandResult>)>, peers: &[Peer], my_id: u64) -> Self {
        let clients: Vec<ScowKeyValueClient<Channel>> = peers.iter()
            .filter(|s| s.id != my_id)
            .flat_map(|p| client_tools::build_client(p))
            .collect();


        tracing::info!("built clients in StateHandler new: {:?}", clients);
        Self {
            rx,
            server_state: Arc::new(Mutex::new(ServerState::new())),
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

    async fn heartbeat(&mut self) -> Vec<Result<AppendEntriesReply, JoinError>> {
        // self.clients
        // TODO: figure out how to make many parallel requests to these clients without blocking.
        // we already had this problem in heartbeat.rs, we may need our own sub-channel to parallelize
        // LETS TRY

        // let server_state = self.server_state.lock().await;

        let mut joinset = JoinSet::new();
        let mut results = vec![];

        for mut client in <Vec<ScowKeyValueClient<Channel>> as Clone>::clone(&self.clients).into_iter() {
            joinset.spawn(async move {
                tracing::info!("sending append_entries HEARTBEAT to {:?}", client);
                client.append_entries(AppendEntriesRequest {
                    leader_term: 54321,
                    leader_id: 12345,
                    prev_log_index: todo!(),
                    prev_log_term: todo!(),
                    leader_commit: todo!(),
                    entries: todo!(),
                }).await;
            });
        };

        while let Some(res) = joinset.join_next().await {
            results.push(res);
        }

        results
    }

    async fn handle_command(&mut self, cmd: StateCommand, response_channel: oneshot::Sender<StateCommandResult>) -> Result<(), String> {
        match cmd {
            StateCommand::GetServerState => {
                tracing::info!("StateHandler got a GetServerState command. 📖");
                let state = self.server_state.lock().await.clone();
                response_channel.send(StateCommandResult::StateResponse(state));
            },
            StateCommand::SetServerState(s) => {
                self.set_server_state(s).await;
                response_channel.send(StateCommandResult::StateSuccess);
            },
            StateCommand::Heartbeat => {
                let results = self.heartbeat().await;
                response_channel.send(StateCommandResult::HeartbeatResponse(results));
            }
        }
        Ok(())
    }

}