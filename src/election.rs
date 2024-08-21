use std::{error::Error, sync::Arc, time::Duration};

use rand::{thread_rng, Rng};
use tokio::sync::{mpsc::Sender, oneshot};

use crate::{
    scow_impl::{LeaderState, Role},
    state_handler::{StateCommand, StateCommandResult},
    Config,
};

#[path = "./client_tools.rs"]
mod client_tools;

pub struct ElectionHandler {
    command_handler_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>,
    config: Arc<Config>,
    // id: u64,
}

impl ElectionHandler {
    pub fn new(
        command_handler_tx: Sender<(StateCommand, oneshot::Sender<StateCommandResult>)>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            command_handler_tx: command_handler_tx,
            config: config,
        }
    }
    pub async fn run_election_loop(&self) -> Result<(), Box<dyn Error>> {
        self.election_loop().await?;
        Ok(())
    }

    async fn election_loop(&self) -> Result<(), Box<dyn Error>> {
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
                self.command_handler_tx
                    .send((StateCommand::GetServerState, response_tx))
                    .await
                    .ok()
                    .unwrap(); //todo 💣
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
                    server_state_inner.voted_for = None;
                    tracing::info!(
                        "incrementing TERM ➕➕ to {} and setting role to Candidate",
                        server_state_inner.current_term
                    );

                    let (state_update_tx, state_update_rx) = oneshot::channel();
                    self.command_handler_tx
                        .send((
                            StateCommand::SetServerState(server_state_inner),
                            state_update_tx,
                        ))
                        .await?;

                    match state_update_rx.await {
                        Ok(update) => tracing::info!(
                            "got a result from updating state in election: {:?}",
                            update
                        ),
                        Err(e) => {
                            tracing::error!("got an ERR from updating state in election: {:?}", e)
                        }
                    }

                    let (request_vote_tx, request_vote_rx) = oneshot::channel();
                    self.command_handler_tx
                        .send((StateCommand::RequestVote, request_vote_tx))
                        .await?;

                    match request_vote_rx.await {
                        Ok(cmd_res) => {
                            tracing::info!(
                                "got a result from requesting votes in election: {:?}",
                                cmd_res
                            );
                            match cmd_res {
                                StateCommandResult::RequestVoteResponse(vote_res) => {
                                    // how many votes do we need?!?
                                    let vote_threshold = self.config.servers.len() / 2 + 1;
                                    // the requester "votes for itself", can we just say we automatically have 1 vote?
                                    // Or do we need to request_vote from ourselves? We don't have a client to ourselves right now.

                                    server_state_inner.voted_for = Some(server_state_inner.id);

                                    let granted_votes = vote_res.into_iter().fold(1, |acc, x| {
                                        if let Ok(vote) = x {
                                            let vote = vote.into_inner();
                                            if vote.vote_granted {
                                                acc + 1
                                            } else {
                                                acc
                                            }
                                        } else {
                                            acc // todo this logic flow is horrendous
                                        }
                                    });
                                    if granted_votes >= vote_threshold {
                                        // we win!
                                        tracing::info!("WE WIN ✌️✌️✌️✌️✌️✌️");
                                        server_state_inner.role = Role::Leader;
                                        server_state_inner.leader_state = Some(LeaderState::new());
                                        // commit the state change
                                        let (state_update_tx, state_update_rx) = oneshot::channel();
                                        self.command_handler_tx
                                            .send((
                                                StateCommand::SetServerState(server_state_inner),
                                                state_update_tx,
                                            ))
                                            .await?;

                                        match state_update_rx.await {
                                            Ok(update) => tracing::info!(
                                                "got a result from updating state after winning election: {:?}",
                                                update
                                            ),
                                            Err(e) => {
                                                tracing::error!("got an ERR from updating state after winning election: {:?}", e)
                                            }
                                        }
                                    } else {
                                        // we don't win!
                                        tracing::info!("WE DO NOT WIN 😢😢😢😢😢😢");
                                    }

                                    // // reset heartbeat timer maybe? These elections are chaotic rn
                                    // server_state_inner.last_heartbeat = Instant::now();
                                }
                                _ => {
                                    tracing::error!("got the wrong response type from vote req.")
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("got an ERR from requesting votes in election: {:?}", e)
                        }
                    }
                }
            };
        }
    }
}
