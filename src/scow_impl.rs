use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::task::JoinSet;
use tokio::time::Instant;
use tonic::transport::Channel;
use tonic::{Request, Response, Status};

use crate::db::Db;
use crate::scow_key_value_client::ScowKeyValueClient;
use crate::{scow::*, Config, Peer};
use crate::scow_key_value_server::ScowKeyValue;

#[derive(Debug, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Follower, 
    Leader, 
    Candidate
}

pub struct ServerState {
    pub role: Role,
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub last_heartbeat: Instant,
    pub last_log_index: u64,
    pub leader_state: Option<LeaderState>,
}

impl ServerState {
    fn new() -> Self {
        Self {
            role: Role::Follower,
            current_term: 0,
            voted_for: None,
            last_heartbeat: Instant::now(), // this could cause problems.
            last_log_index: 0,
            leader_state: None
        }
    }
}

pub struct LeaderState {
    /// for each server, index of the next log entry to send to that server (initialized to leaderlast log index + 1)
    next_index: HashMap<u64, u64>,
    /// for each server, index of highest log entry known to be replicated on server (initialized to 0, increases monotonically)
    match_index: HashMap<u64, u64>,
}

pub struct MyScowKeyValue {
    db: Db,
    peers: Vec<Peer>,
    pub server_state: Arc<Mutex<ServerState>>,
}

#[tonic::async_trait]
impl ScowKeyValue for MyScowKeyValue {
    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusReply>, Status> {
        println!("Got a request from {:?}", request.remote_addr());

        let reply = StatusReply {
            status: "Status: OK".to_string(),
        };
        Ok(Response::new(reply))
    }

    async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetReply>, Status> {
        let db_response = self.db.get(request.into_inner().key.as_str());

        match db_response {
            Some(s) => {
                Ok(Response::new(GetReply {
                    value: s
                }))
            },
            None => Err(Status::not_found("key not found")),
        }        
    }

    async fn set(&self, request: Request<SetRequest>) -> Result<Response<SetReply>, Status> {
        let inner = request.into_inner();
        let _db_response = self.db.set(&inner.key, &inner.value);
        Ok(Response::new(SetReply { success: true}))
    }

    async fn append_entries(&self, request: Request<AppendEntriesRequest>) -> Result<Response<AppendEntriesReply>, Status> {
        let inner = request.into_inner();
        for i in inner.entries {
            self.db.set(&i.key, &i.value);
        }

        let mut state_inner = self.server_state.lock().unwrap();
        state_inner.last_heartbeat = Instant::now();
        Ok(Response::new(AppendEntriesReply { term: 0, success: true}))
    }

    async fn request_vote(&self, request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteReply>, Status> {
        let inner = request.into_inner();
        let server_result = self.server_request_vote(
            inner.term, 
            inner.candidate_id, 
            inner.last_log_index
        );
        Ok(Response::new(RequestVoteReply { term: server_result.0, vote_granted: server_result.1 }))
    }
}

impl MyScowKeyValue {
    pub fn new() -> Self {
        MyScowKeyValue {
            db: Db::new(),
            peers: vec![],
            server_state: Arc::new(Mutex::new(ServerState {
                role: Role::Follower,
                current_term: 0,
                voted_for: None,
                last_heartbeat: Instant::now(),
                last_log_index: 0,
                leader_state: None,
            }))
        }
    }

    pub fn server_request_vote(&self, candidate_term: u64, candidate_id: u64, candidate_last_index: u64) -> (u64, bool) {
        let mut server_state = self.server_state.lock().unwrap();
        if candidate_term < server_state.current_term {
            server_state.voted_for = None;
            (server_state.current_term, false)
        } else {
            if (server_state.voted_for == None || server_state.voted_for == Some(candidate_id)) &&
            candidate_last_index >= server_state.last_log_index {
                server_state.voted_for = Some(candidate_id);
                (server_state.current_term, true)
            } else {
                server_state.voted_for = None;
                (server_state.current_term, false)
            }
        }
    }

    // TODO should I move all the heartbeat and voting stuff here? Should make state and futures easier to manage.

    pub async fn election_loop_doer(&self, config_arc: Arc<Config>, my_id: u64) -> Result<(), Box<dyn Error>> {
        self.election_loop(config_arc, my_id).await;
        Ok(())
    }

    async fn election_loop(&self, config_arc: Arc<Config>, my_id: u64) -> () {
        let mut peer_clients: Vec<ScowKeyValueClient<Channel>> = vec![];
        let peer_configs: Vec<&Peer> = config_arc.servers.iter().filter(|s| s.id != my_id).collect();
        
    
        for p in peer_configs.iter() {
            let client = Self::build_client(p).await;
            match client {
                Ok(c) => peer_clients.push(c),
                Err(_) => panic!("asdfasdfd"),
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
