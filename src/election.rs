use std::{error::Error, future::Future, sync::Arc, thread, time::Duration};

use futures::stream::FuturesUnordered;
use rand::{thread_rng, Rng};
use tokio::{
    sync::Mutex,
    task::{JoinError, JoinSet},
    time::{timeout, Instant},
};
use tonic::transport::Channel;

use crate::{
    scow,
    scow_impl::{LeaderState, Role, ServerState},
    scow_key_value_client::ScowKeyValueClient,
    Config, Peer, RequestVoteReply, RequestVoteRequest,
};

#[path = "./client_tools.rs"]
mod client_tools;

pub struct ElectionHandler {
    server_state: Arc<Mutex<ServerState>>,
    config: Arc<Config>,
    id: u64,
}

impl ElectionHandler {
    pub fn new(server_state: Arc<Mutex<ServerState>>, config: Arc<Config>, id: u64) -> Self {
        Self {
            server_state,
            config,
            id,
        }
    }
    pub async fn run_election_loop(&self) -> Result<(), Box<dyn Error>> {
        self.election_loop().await;
        Ok(())
    }

    async fn election_loop(&self) {
        let mut peer_clients: Vec<ScowKeyValueClient<Channel>> = vec![];
        let mut rng = thread_rng();
        let peer_configs: Vec<&Peer> = self
            .config
            .servers
            .iter()
            .filter(|s| s.id != self.id)
            .collect();

        for p in peer_configs.iter() {
            let client = client_tools::build_client(p);
            match client {
                Ok(c) => peer_clients.push(c),
                Err(e) => panic!("panic from build_client: {:?}", e),
            }
        }

        let interval_range =
            self.config.election_timeout_min_ms as u64..self.config.election_timeout_max_ms as u64;

        let mut interval =
            tokio::time::interval(Duration::from_millis(rng.gen_range(interval_range.clone())));
        loop {
            interval.tick().await;
            {
                let mut server_state_inner = self.server_state.lock().await;
                let elapsed = server_state_inner.last_heartbeat.elapsed();
                tracing::info!(
                    "TIME SINCE LAST 💞: {:?}, period: {:?}",
                    elapsed,
                    interval.period()
                );
                if (server_state_inner.role == Role::Follower
                    || server_state_inner.role == Role::Candidate)
                    && elapsed > interval.period()
                // TODO: I think this is wrong, we need to check more often for a heartbeat
                // The way it should work:
                // instead of polling on this long interval:
                // set a timeout that gets canceled and reset each time we get a heartbeat
                {
                    // increment term!
                    server_state_inner.current_term += 1;
                    // set role!
                    server_state_inner.role = Role::Candidate;
                    // Request votes!
                    let vote_res = self
                        .initiate_vote(&server_state_inner, peer_clients.clone())
                        .await;
                    tracing::info!("vote results:{:?}", vote_res);


                    // how many votes do we need?!?
                    let vote_threshold = (peer_clients.len() / 2) + 1;
                    // the requester "votes for itself", can we just say we automatically have 1 vote?
                    // Or do we need to request_vote from ourselves? We don't have a client to ourselves right now.

                    let granted_votes =
                        vote_res
                            .iter()
                            .fold(1, |acc, x| if x.vote_granted { acc + 1 } else { acc });
                    if granted_votes >= vote_threshold {
                        // we win!
                        tracing::info!("WE WIN ✌️✌️✌️✌️✌️✌️");
                        server_state_inner.role = Role::Leader;
                        server_state_inner.leader_state =
                            Some(LeaderState::new(server_state_inner.last_log_index));
                    } else {
                        // we don't win!
                        tracing::info!("WE DO NOT WIN 😢😢😢😢😢😢");
                    }

                    // reset heartbeat timer maybe? These elections are chaotic rn
                    server_state_inner.last_heartbeat = Instant::now();
                }
            };
        }
    }

// try this one at a time?!
    async fn single_vote(
        &self,
        server_state: &ServerState,
        mut client: ScowKeyValueClient<Channel>
    ) -> Option<RequestVoteReply> {
        let res = client.request_vote(RequestVoteRequest {
            term: server_state.current_term,
            candidate_id: self.id,
            last_log_index: 2,
            last_log_term: 3
        }).await;

        match res {
            Ok(r) => Some(r.into_inner()),
            Err(_) => None 
        }
    }


    async fn initiate_vote(
        &self,
        server_state: &ServerState,
        peer_clients: Vec<ScowKeyValueClient<Channel>>,
    ) -> Vec<RequestVoteReply> {
        let mut replies = vec![];

        for mut client in peer_clients {
            tracing::info!("issuing request_vote to {:?}", client);
            let res = client
                .request_vote(RequestVoteRequest {
                    term: server_state.current_term,
                    candidate_id: self.id,
                    last_log_index: 2,
                    last_log_term: 3,
                })
                .await;

            match res {
                Ok(r) => replies.push(r.into_inner()),
                Err(e) => {
                    tracing::error!("err from getting vote reply: {:?}", e)
                }
            }
        }
        replies
    }

    async fn timeout_heartbeat_loop(&self) {
        let mut peer_clients: Vec<ScowKeyValueClient<Channel>> = vec![];
        let mut rng = thread_rng();
        let peer_configs: Vec<&Peer> = self
            .config
            .servers
            .iter()
            .filter(|s| s.id != self.id)
            .collect();

        for p in peer_configs.iter() {
            let client = client_tools::build_client(p);
            match client {
                Ok(c) => peer_clients.push(c),
                Err(e) => panic!("panic from build_client: {:?}", e),
            }
        }

        let interval_range =
            self.config.election_timeout_min_ms as u64..self.config.election_timeout_max_ms as u64;

        let mut interval =
            tokio::time::interval(Duration::from_millis(rng.gen_range(interval_range.clone())));

        let mut this_timeout = timeout(interval.period(), async move {
            // our
        });
    }

}
