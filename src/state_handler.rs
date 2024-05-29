use std::sync::Arc;

use tokio::sync::{mpsc::Receiver, oneshot, Mutex};
use tonic::transport::Channel;

use crate::{scow_impl::ServerState, scow_key_value_client::ScowKeyValueClient};


#[derive(Debug)]
pub enum StateCommand {
    GetServerState,
    SetServerState(ServerState),
}

#[derive(Debug)]
pub enum StateCommandResult {
    StateResponse(ServerState),
    StateSuccess,
}

#[derive(Debug)]
pub enum StateCommandError {
    BasicError(String)
}

pub struct StateHandler {
    rx: Receiver<(StateCommand, oneshot::Sender<StateCommandResult>)>,
    server_state: Arc<Mutex<ServerState>>,
    clients: Vec<ScowKeyValueClient<Channel>>
}

impl StateHandler {
    pub fn new(rx: Receiver<(StateCommand, oneshot::Sender<StateCommandResult>)>) -> Self {
        Self {
            rx,
            server_state: Arc::new(Mutex::new(ServerState::new())),
            clients: vec![],
        }
    }

    pub async fn run(&mut self) -> Result<(), StateCommandError> {
        while let Some((cmd, response_channel)) = self.rx.recv().await {
            self.handle_command(cmd, response_channel).await;
        }
        Ok(())
    }

    async fn set_server_state(&mut self, new_state: ServerState) -> Result<(), StateCommandError> {
        let mut current_state = self.server_state.lock().await;
        
        *current_state = new_state;
        Ok(())
    }

    async fn get_server_state(&mut self) -> Result<ServerState, StateCommandError> {
        let current_state = self.server_state.lock().await;
        Ok((*current_state).clone())
    }

    async fn handle_command(&mut self, cmd: StateCommand, response_channel: oneshot::Sender<StateCommandResult>) -> Result<(), StateCommandError> {
        match cmd {
            StateCommand::GetServerState => {
                tracing::info!("StateHandler got a GetServerState command.");
                response_channel.send(StateCommandResult::StateResponse(self.get_server_state().await.unwrap())).expect(
                    "Failed to send server state response from handle_command#GetServerState."
                );
                Ok(())
            },
            StateCommand::SetServerState(s) => {
                self.set_server_state(s).await?;
                response_channel.send(StateCommandResult::StateSuccess).expect(
                    "Failed to send StateSuccess from handle_command#SetServerState."
                );
                Ok(())
            }
        }
    }

}