
use tonic::{transport::Server, Request, Response, Status};
use clap::Parser;

use serde::Deserialize;

use scow::*;
use scow::scow_key_value_server::{ScowKeyValueServer, ScowKeyValue};

mod db;
use crate::db::DbDropGuard;

#[derive(Parser)]
struct Cli {
    #[arg(long = "id")]
    id: u64,
}

#[derive(Deserialize, Debug)]
struct Config {
    servers: Vec<Peer>
}

#[derive(Deserialize, Debug, Clone)]
struct Peer {
    id: u64,
    address: String
}

#[derive(Default)]
pub struct MyScowKeyValue {
    db_drop_guard: DbDropGuard,
    peers: Vec<Peer>
}

pub mod scow {
    tonic::include_proto!("scow");
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
            None => Err(Status::not_found("key not found")),
        }        
    }

    async fn set(&self, request: Request<SetRequest>) -> Result<Response<SetReply>, Status> {
        let inner = request.into_inner();
        let _db_response = self.db_drop_guard.db().set(inner.key, inner.value);
        Ok(Response::new(SetReply { success: true}))
    }

    async fn append_entries(&self, request: Request<AppendEntriesRequest>) -> Result<Response<AppendEntriesReply>, Status> {
        Err(Status::permission_denied("append_entires impl missing"))
    }
    async fn request_vote(&self, request: Request<RequestVoteRequest>) -> Result<Response<RequestVoteReply>, Status> {
        Err(Status::permission_denied("request_vote impl missing"))
    }

}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let cli = Cli::parse();

    let config_file = std::fs::File::open("config.yaml")?;
    let config: Config = serde_yaml::from_reader(config_file)?;

    let my_config = config.servers.iter().find(|s| s.id == cli.id).expect("couldn't find my config.");
    let peer_configs: Vec<&Peer> = config.servers.iter().filter(|s| s.id != cli.id).collect();
    
    println!("MY config: {:?}", my_config);
    println!("PEER configs: {:?}", peer_configs);

    let addr = my_config.address.parse().unwrap();
    let scow_key_value1 = MyScowKeyValue::default();

    let s1 = Server::builder()
        .add_service(ScowKeyValueServer::new(scow_key_value1))
        .serve(addr)
        .await?;
    
    Ok(())
}