use tonic::{transport::Server, Request, Response, Status};
use scow::{PingRequest, PingReply};
use scow::scow_key_value_server::{ScowKeyValueServer, ScowKeyValue};

pub mod scow {
    tonic::include_proto!("scow");
}

#[derive(Default)]
pub struct MyScowKeyValue {}

#[tonic::async_trait]
impl ScowKeyValue for MyScowKeyValue {
    async fn ping(
        &self,
        request: Request<PingRequest>,
    ) -> Result<Response<PingReply>, Status> {
        println!("Got a request from {:?}", request.remote_addr());

        let reply = PingReply {
            greeting: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse().unwrap();
    let greeter = MyScowKeyValue::default();

    println!("GreeterServer listening on {}", addr);

    Server::builder()
        .add_service(ScowKeyValueServer::new(greeter))
        .serve(addr)
        .await?;

    Ok(())
}