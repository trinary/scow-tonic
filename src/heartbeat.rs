use std::{error::Error, sync::Arc, time::Duration};

use tokio::{
    sync::{mpsc::Sender, oneshot},
    time::Instant,
};

use crate::{
    scow_impl::Role,
    state_handler::{StateCommand, StateCommandResult},
    Config,
};

pub struct Heartbeat {
    command_handler_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>,
    config: Arc<Config>,
}

impl Heartbeat {
    pub fn new(
        command_handler_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            command_handler_tx,
            config,
        }
    }

    pub async fn run_heartbeat_loop(&self) -> Result<(), Box<dyn Error>> {
        self.heartbeat_loop().await?;
        Ok(())
    }

    async fn heartbeat_loop(&self) -> Result<(), Box<dyn Error>> {
        let mut heartbeat_interval = tokio::time::interval(Duration::from_millis(
            self.config.heartbeat_interval_ms.into(),
        ));
        heartbeat_interval.tick().await;

        loop {
            heartbeat_interval.tick().await;
            {
                tracing::info!("asking for server_state in heartbeat_loop inner");
                let (response_tx, response_rx) = oneshot::channel();
                self.command_handler_tx
                    .send((StateCommand::GetServerState, response_tx))
                    .await?; //todo resultify
                let state_result = response_rx.await?;

                let mut server_state_inner = match state_result {
                    StateCommandResult::StateResponse(state) => state,
                    _ => panic!("got the wrong result from a state read op in heartbeat loop"),
                };

                if server_state_inner.role == Role::Leader {
                    server_state_inner.last_heartbeat = Instant::now(); // assume we are up to date so we don't trigger an election on ourselves....idk about this.
                    tracing::info!(
                        "HEARTBEAT GOING OUT, term {:?} 💖💖💖💖💖",
                        &server_state_inner.current_term
                    );

                    let (heartbeat_response_tx, heartbeat_response_rx) = oneshot::channel();
                    self.command_handler_tx
                        .send((StateCommand::Heartbeat, heartbeat_response_tx))
                        .await?;
                    let heartbeat_response = heartbeat_response_rx.await.unwrap();
                    match heartbeat_response {
                        StateCommandResult::HeartbeatResponse(results) => {
                            tracing::info!("got a heartbeat response {:?}", results);
                        }
                        _ => panic!("got the wrong result from a heartbeat op in heartbeat loop"),
                    };
                }
            };
        }
    }
}
