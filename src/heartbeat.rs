use std::{error::Error, sync::Arc, time::Duration};

use tokio::sync::{Mutex, MutexGuard};
use tonic::transport::Channel;

use crate::{scow_impl::{Role, ServerState}, scow_key_value_client::ScowKeyValueClient, AppendEntriesReply, AppendEntriesRequest, Config, Peer};

#[path="./client_tools.rs"]
mod client_tools;

pub struct Heartbeat {
    server_state: Arc<Mutex<ServerState>>,
    config: Arc<Config>,
    id: u64
}

impl Heartbeat {
    pub fn new(server_state: Arc<Mutex<ServerState>>, config: Arc<Config>, id: u64) -> Self {
        Self {
            server_state: server_state,
            config: config,
            id: id,
        }
    }


    pub async fn run_heartbeat_loop(&self) -> Result<(), Box<dyn Error>> {
        self.heartbeat_loop().await;
        Ok(())
    }

    async fn heartbeat_loop(&self) -> () {
        let mut peer_clients: Vec<ScowKeyValueClient<Channel>> = vec![];

        let peer_configs: Vec<&Peer> = self.config.servers.iter().filter(|s| s.id != self.id).collect();

        for p in peer_configs.iter() {
            let client = client_tools::build_client(p);
            match client {
                Ok(c) => peer_clients.push(c),
                Err(e) => {
                    tracing::error!("failed to build client: {:?}", e);
                } 
            }
        }

        let mut heartbeat_interval = tokio::time::interval(Duration::from_millis(500));
        heartbeat_interval.tick().await;

        loop {
            heartbeat_interval.tick().await;
            tracing::debug!("heartbeat loop run");

            let _x = {
                let mut server_state_inner = self.server_state.lock().await;
                tracing::debug!("got lock on server state inside heartbeat loop anonymous block");

                if server_state_inner.role == Role::Leader {
                    // we are the leader, issue AppendEntries heartbeats to peers
                    Self::heartbeat_request(peer_clients.clone(), &server_state_inner, self.id).await;

                }
            };
        }
    }

    async fn heartbeat_request(peer_clients: Vec<ScowKeyValueClient<Channel>>, server_state: &ServerState, id: u64) -> Vec<AppendEntriesReply> {
        let mut replies = vec![];
        for mut client in peer_clients {
            let res = client.append_entries(AppendEntriesRequest {
                leader_term: server_state.current_term,
                leader_id: id,
                prev_log_index: 2,
                prev_log_term: 3,
                leader_commit: 1,
                entries: vec![],
            }).await;

            match res {
                Ok(r) => replies.push(r.into_inner()),
                Err(e) => {
                    tracing::error!("err from heartbeat AppendEntries request: {:?}", e)
                }
            }
        }
        replies
    }
}