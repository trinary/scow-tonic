
use tonic::{Request, Response, Status};

use crate::db::DbDropGuard;
use crate::{scow::*, Peer};
use crate::scow_key_value_server::ScowKeyValue;

pub struct MyScowKeyValue {
    db_drop_guard: DbDropGuard,
    peers: Vec<Peer>
}

// pub mod scow {
//     tonic::include_proto!("scow");
// }

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
        let db_response = self.db_drop_guard.db().get(request.into_inner().key.as_str());

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
        let _db_response = self.db_drop_guard.db().set(&inner.key, &inner.value);
        Ok(Response::new(SetReply { success: true}))
    }

    async fn append_entries(&self, request: Request<AppendEntriesRequest>) -> Result<Response<AppendEntriesReply>, Status> {
        let inner = request.into_inner();
        for i in inner.entries {
            self.db_drop_guard.db().set(&i.key, &i.value);
        }
        Ok(Response::new(AppendEntriesReply { term: 0, success: true}))
    }
    async fn request_vote(&self, request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteReply>, Status> {
        Err(Status::permission_denied("request_vote impl missing"))
    }
}

impl MyScowKeyValue {
    pub fn new() -> Self {
        MyScowKeyValue {
            db_drop_guard: DbDropGuard::new(),
            peers: vec![],
        }
    }
}
