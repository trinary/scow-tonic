use std::{error::Error, sync::Arc, time::Duration};

use tokio::sync::{mpsc::Sender, oneshot};
use rand::{thread_rng, Rng};
use tokio::{
    sync::Mutex,
    task::{JoinError, JoinSet},
    time::{timeout, Instant},
};
use tonic::transport::Channel;

use crate::{
    scow, scow_impl::{LeaderState, Role, ServerState}, scow_key_value_client::ScowKeyValueClient, state_handler::{StateCommand, StateCommandResult}, Config, Peer, RequestVoteReply, RequestVoteRequest
};

#[path = "./client_tools.rs"]
mod client_tools;

pub struct ElectionHandler {
    command_handler_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>,
    config: Arc<Config>,
    // id: u64,
}

impl ElectionHandler {
    pub fn new(command_handler_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>, config: Arc<Config>) -> Self {
        Self {
            command_handler_tx: command_handler_tx,
            config: config,
        }
    }
    pub async fn run_election_loop(&self) -> Result<(), Box<dyn Error>> {
        self.election_loop().await;
        Ok(())
    }

    async fn election_loop(&self) {
        let mut rng = thread_rng();
        
        let interval_range =
            self.config.election_timeout_min_ms as u64..self.config.election_timeout_max_ms as u64;

        let mut interval =
            tokio::time::interval(Duration::from_millis(rng.gen_range(interval_range.clone())));
        loop {
            interval.tick().await;
            {
                tracing::info!("asking for server_state in election loop inner");
                let (response_tx, response_rx) = oneshot::channel();
                self.command_handler_tx.send((StateCommand::GetServerState, response_tx)).await.ok().unwrap(); //todo 💣
                let state_result = response_rx.await.unwrap();

                let mut server_state_inner = match state_result {
                    StateCommandResult::StateResponse(state) => state,
                    _ => panic!("got the wrong result from a state read op in heartbeat loop"),
                };
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
                    // here we will COMMIT a changed server state before initiating a vote.

                    server_state_inner.current_term += 1; // TODO we need to account for this when calling the channel handler?
                    // set role!
                    server_state_inner.role = Role::Candidate;

                    let (state_update_tx, state_update_rx) = oneshot::channel();
                    self.command_handler_tx.send((StateCommand::SetServerState(server_state_inner), state_update_tx)).await;

                    match state_update_rx.await {
                        Ok(update) => tracing::info!("got a result from updating state in election: {:?}", update),
                        Err(e) => tracing::error!("got an ERR from updating state in election: {:?}", e),
                    }

                    let (request_vote_tx, request_vote_rx) = oneshot::channel();
                    self.command_handler_tx.send((StateCommand::RequestVote, request_vote_tx)).await;

                    match request_vote_rx.await {
                        Ok(vote_res) => tracing::info!("got a result from requesting votes in election: {:?}", vote_res),
                        Err(e) => tracing::error!("got an ERR from requesting votes in election: {:?}", e),
                    }

                    //let vote_request_result = request_vote_rx.await.unwrap();
                    // Request votes!
                    //tracing::info!("vote results:{:?}", vote_request_result);


                    // how many votes do we need?!?
                    // let vote_threshold = self.config.servers.len() / 2 + 1;
                    // the requester "votes for itself", can we just say we automatically have 1 vote?
                    // Or do we need to request_vote from ourselves? We don't have a client to ourselves right now.

                    // let granted_votes = 1;
                        // vote_request_result
                        //     .iter()
                        //     .fold(1, |acc, x| if x.vote_granted { acc + 1 } else { acc });
                    // if granted_votes >= vote_threshold {
                    //     // we win!
                    //     tracing::info!("WE WIN ✌️✌️✌️✌️✌️✌️");
                    //     server_state_inner.role = Role::Leader;
                    //     server_state_inner.leader_state =
                    //         Some(LeaderState::new(server_state_inner.clone().last_log_index));
                    // } else {
                    //     // we don't win!
                    //     tracing::info!("WE DO NOT WIN 😢😢😢😢😢😢");
                    // }

                    // // reset heartbeat timer maybe? These elections are chaotic rn
                    // server_state_inner.last_heartbeat = Instant::now();
                }
            };
        }
    }

}
