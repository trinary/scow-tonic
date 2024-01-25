
use tonic::{transport::Server, Request, Response, Status};
use scow::{GetRequest, GetReply, SetRequest, SetReply, StatusReply, StatusRequest};
use scow::scow_key_value_server::{ScowKeyValueServer, ScowKeyValue};

mod db;
use crate::db::DbDropGuard;

pub mod scow {
    tonic::include_proto!("scow");
}

#[derive(Default)]
pub struct MyScowKeyValue {
    db_drop_guard: DbDropGuard,
}

#[tonic::async_trait]
impl ScowKeyValue for MyScowKeyValue {
    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusReply>, Status> {
        println!("Got a request from {:?}", request.remote_addr());

        let reply = StatusReply {
            status: format!("Status: OK"),
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
            None => Err(Status::not_found("message")),
        }        
    }

    async fn set(&self, request: Request<SetRequest>) -> Result<Response<SetReply>, Status> {
        let inner = request.into_inner();
        let _db_response = self.db_drop_guard.db().set(inner.key, inner.value);
        Ok(Response::new(SetReply { success: true}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse().unwrap();
    let scow_key_value = MyScowKeyValue::default();

    println!("GreeterServer listening on {}", addr);

    Server::builder()
        .add_service(ScowKeyValueServer::new(scow_key_value))
        .serve(addr)
        .await?;

    Ok(())
}