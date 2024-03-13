use std::{error::Error, sync::{Arc, Mutex}, time::Duration};

use rand::{thread_rng, Rng};
use tonic::transport::Channel;

use crate::{scow_impl::{Role, ServerState}, scow_key_value_client::ScowKeyValueClient, Config, Peer, RequestVoteReply, RequestVoteRequest};



pub struct ElectionHandler {
    server_state: Arc<Mutex<ServerState>>,
    config: Arc<Config>,
    id: u64,
}

impl ElectionHandler {
    pub fn new(server_state: Arc<Mutex<ServerState>>, config_arc: Arc<Config>, id: u64) -> Self {
        Self {
            server_state: server_state,
            config: config_arc,
            id: id
        }
    }
    pub async fn election_loop_doer(&self) -> Result<(), Box<dyn Error>> {
        self.election_loop().await;
        Ok(())
    }

    async fn election_loop(&self) -> () {
        let mut peer_clients: Vec<ScowKeyValueClient<Channel>> = vec![];
        let mut rng = thread_rng();
        let peer_configs: Vec<&Peer> = self.config.servers.iter().filter(|s| s.id != self.id).collect();
        
    
        for p in peer_configs.iter() {
            let client = Self::build_client(p).await;
            match client {
                Ok(c) => peer_clients.push(c),
                Err(e) => panic!("panic from build_client: {:?}", e),
            }
        }

        let interval_range = self.config.election_timeout_min_ms as u64..self.config.election_timeout_max_ms as u64;
    
        let timeout = Duration::from_millis(1500);
        loop {
            let mut interval = tokio::time::interval(Duration::from_millis(rng.gen_range(interval_range.clone())));
            interval.tick().await;
            tracing::debug!("requesting lock inside election loop anonymous block");
            let _x = {
                //let mut server_state_inner = server_state.lock().unwrap();
                tracing::debug!("got lock on server state inside election loop anonymous block, initiating votes maybe.");
                //if server_state_inner.role == Role::Follower {
                //    if server_state_inner.last_heartbeat.elapsed() > timeout {
                        // Request votes!
                        let vote_res = Self::initiate_vote(peer_clients.clone()).await;
                        tracing::info!("vote results:{:?}", vote_res);
                //    }
                //}
                tracing::debug!("got to end of inner vote loop anonymous block.");
            };
        }
    }

    async fn initiate_vote(peer_clients: Vec<ScowKeyValueClient<Channel>>) -> Vec<RequestVoteReply> {
        let mut replies = vec![];

        for mut client in peer_clients {
            let res = client.request_vote(RequestVoteRequest {
                term: 1,
                candidate_id: 1,
                last_log_index: 2,
                last_log_term: 3,
            }).await;
    
            match res {
                Ok(r) => replies.push(r.into_inner()),
                Err(e) => { 
                    tracing::error!("err from getting vote reply: {:?}", e)
                },
            }
        }
        replies
    }

    async fn build_client(peer: &Peer) -> Result<ScowKeyValueClient<Channel>, Box<dyn std::error::Error>> {
        if let Ok(uri) = peer.uri.parse() {
            let endpoint = tonic::transport::channel::Channel::builder(uri);
            Ok(ScowKeyValueClient::new(endpoint.connect_lazy()))
        } else {
            Err("invalid uri")?
        }
    }
}