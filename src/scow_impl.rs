use std::borrow::BorrowMut;
use std::sync::{Arc, Mutex};

use tokio::time::Instant;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

use crate::db::Db;
use crate::{scow::*, Peer};
use crate::scow_key_value_server::ScowKeyValue;

#[derive(Debug, Default)]
enum Role {
    #[default]
    Follower, 
    Leader, 
    Candidate
}

pub struct ServerState {
    role: Role,
    current_term: u64,
    voted_for: Option<u64>,
    last_heartbeat: Instant,
    last_log_index: u64,
}

pub struct MyScowKeyValue {
    db: Db,
    peers: Vec<Peer>,
    server_state: Arc<Mutex<ServerState>>,
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
        //self.db.update_heartbeat()?;
        Ok(Response::new(AppendEntriesReply { term: 0, success: true}))
    }

    async fn request_vote(&self, request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteReply>, Status> {
        let inner = request.into_inner();
        // TODO : we can't change this signature to &mut self, that's defined by tonic.
        // SO how do we represent this write operation on MyScowKeyValue::sever_state to fix the error on self.server_request_vote?
        // Is server_state in the wrong place? Who should own it? 

        // Look at tonic examples for clues. Didnt find any immediately.
        // "interior mutability" is maybe the term that I'm running into here. not sure if I want to get into using refcell stuff to fix this

        // after further consideration, we'll need to do refcell stuff to fix this. Mutex it?


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
            }))
        }
    }

    pub fn server_request_vote(&self, candidate_term: u64, candidate_id: u64, candidate_last_index: u64) -> (u64, bool) {
        // wrap access to server_state in mutex lock(), and it should be good, idk how that actually works tho because it won't be mut!
        let mut server_state = self.server_state.lock().unwrap();
        // it just works?!!?! fuck
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
}
