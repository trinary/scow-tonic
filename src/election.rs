use std::{error::Error, sync::{Arc, Mutex}, time::Duration};

use tokio::task::JoinSet;
use tonic::transport::Channel;

use crate::{scow_impl::{Role, ServerState}, scow_key_value_client::ScowKeyValueClient, Config, Peer, RequestVoteReply, RequestVoteRequest};



pub struct ElectionHandler {
    server_state: Arc<Mutex<ServerState>>,
    config: Arc<Config>,
    id: u64,
}

impl ElectionHandler {
    pub fn new(server_state: &Arc<Mutex<ServerState>>, config_arc: Arc<Config>, id: u64) -> Self {
        Self {
            server_state: server_state.clone(),
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
        let peer_configs: Vec<&Peer> = self.config.servers.iter().filter(|s| s.id != self.id).collect();
        
    
        for p in peer_configs.iter() {
            let client = Self::build_client(p).await;
            match client {
                Ok(c) => peer_clients.push(c),
                Err(e) => panic!("panic from build_client: {:?}", e),
            }
        }
        println!("peer clients?!!? {:?}", peer_clients);
    
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        let mut timeout = Duration::from_millis(500);
        println!("gonna start the election/vote-request loop.");
        loop {
            interval.tick().await;
            let server_state_inner = self.server_state.lock().unwrap();
            if server_state_inner.role == Role::Follower {
                if server_state_inner.last_heartbeat.elapsed() > timeout {
                    // Request votes!
                    let vote_res = Self::initiate_vote(peer_clients.clone()).await;
                    println!("vote results:{:?}", vote_res);
                }
            }
        }
    }

    async fn initiate_vote(peer_clients: Vec<ScowKeyValueClient<Channel>>) -> Vec<RequestVoteReply> {
        println!("initiated vote request???");
        let mut set = JoinSet::new();
        let mut replies = vec![];
    
        // need current term, log index, log term (log term??)
        for mut client in peer_clients {
            set.spawn(async move {
                client.request_vote(RequestVoteRequest {
                    term: 1,
                    candidate_id: 1,
                    last_log_index: 2,
                    last_log_term: 3,
                }).await
            });
        }

        while let Some(res) = set.join_next().await {
            let reply = res.unwrap();
            match reply {
                Ok(r) => replies.push(r.into_inner()),
                Err(e) => println!("err from getting vote reply: {:?}", e),
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