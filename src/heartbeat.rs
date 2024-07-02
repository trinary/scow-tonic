use std::{error::Error, sync::Arc, time::Duration};

use tokio::{sync::{mpsc::Sender, oneshot}, time::Instant};
use tonic::transport::Channel;

use crate::{
    scow_impl::{Role, ServerState}, scow_key_value_client::ScowKeyValueClient, state_handler::{StateCommand, StateCommandResult}, AppendEntriesReply, AppendEntriesRequest, Config, Peer
};

#[path = "./client_tools.rs"]
mod client_tools;

pub struct Heartbeat {
    command_handler_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>,
    config: Arc<Config>,
}

impl Heartbeat {
    pub fn new(command_handler_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>, config: Arc<Config>) -> Self {
        Self {
            command_handler_tx: command_handler_tx,
            config: config,
        }
    }

    pub async fn run_heartbeat_loop(&self) -> Result<(), Box<dyn Error>> {
        self.heartbeat_loop().await;
        Ok(())
    }

    async fn heartbeat_loop(&self) -> () {

        let mut heartbeat_interval = tokio::time::interval(Duration::from_millis(
            self.config.heartbeat_interval_ms.into(),
        ));
        heartbeat_interval.tick().await;

        loop {
            heartbeat_interval.tick().await;
            {
                tracing::info!("asking for server_state in heartbeat_loop inner");
                let (response_tx, response_rx) = oneshot::channel();
                self.command_handler_tx.send((StateCommand::GetServerState, response_tx)).await.ok().unwrap(); //todo 💣
                let state_result = response_rx.await.unwrap();

                let mut server_state_inner = match state_result {
                    StateCommandResult::StateResponse(state) => state,
                    _ => panic!("got the wrong result from a state read op in heartbeat loop"),
                };

                if server_state_inner.role == Role::Leader {
                    // we are the leader
                    server_state_inner.last_heartbeat = Instant::now(); // assume we are up to date so we don't trigger an election on ourselves....idk about this.
                    tracing::info!(
                        "HEARTBEAT GOING OUT, term {:?} 💖💖💖💖💖",
                        &server_state_inner.current_term
                    );

                    let (heartbeat_response_tx, heartbeat_response_rx) = oneshot::channel();                    self.command_handler_tx.send((StateCommand::Heartbeat, heartbeat_response_tx)).await.ok().unwrap();
                    let heartbeat_response = heartbeat_response_rx.await.unwrap();
                    let heartbeat_replies = match heartbeat_response {
                        StateCommandResult::HeartbeatResponse(results) => {
                            tracing::info!("got a heartbeat response {:?}", results);
                        },
                        _ => panic!("got the wrong result from a heartbeat op in heartbeat loop"),
                    };

                    // issue AppendEntries heartbeats to peers
                    // let heartbeat_replies =
                    //     Self::heartbeat_request(peer_clients.clone(), &server_state_inner, self.id)
                    //         .await;

                    // is anyone ahead of us?
                    // for reply in heartbeat_replies {
                    //     if reply.term >= server_state_inner.current_term {
                    //         server_state_inner.current_term = reply.term;
                    //         server_state_inner.role = Role::Follower;
                    //     }
                    // }
                }
            };
        }
    }

    async fn heartbeat_request(
        peer_clients: Vec<ScowKeyValueClient<Channel>>,
        server_state: &ServerState,
        id: u64,
    ) -> Vec<AppendEntriesReply> {
        let mut replies = vec![];
        for mut client in peer_clients {
            let res = client
                .append_entries(AppendEntriesRequest {
                    leader_term: server_state.current_term,
                    leader_id: id,
                    prev_log_index: 2,
                    prev_log_term: 3,
                    leader_commit: 1,
                    entries: vec![],
                })
                .await;

            match res {
                Ok(r) => {
                    tracing::info!("got heartbeat result: {:?}", r);
                    replies.push(r.into_inner())
                }
                Err(e) => {
                    tracing::error!("err from heartbeat AppendEntries request: {:?}", e)
                }
            }
        }
        replies
    }
}
