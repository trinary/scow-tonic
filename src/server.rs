use std::sync::{Arc, Mutex};
use std::time::Duration;

use scow_impl::ServerState;
use tokio::task::JoinSet;
use tokio::join;
use tonic::server;
use tonic::transport::{Channel, Server};
use clap::Parser;

use serde::Deserialize;

use scow::*;
use scow::scow_key_value_server::ScowKeyValueServer;

mod db;

mod scow_impl;
use crate::scow_key_value_client::ScowKeyValueClient;
use crate::scow_impl::{MyScowKeyValue, Role};

pub mod scow {
    tonic::include_proto!("scow");
}

#[derive(Parser)]
struct Cli {
    #[arg(long = "id")]
    id: u64,
}

#[derive(Deserialize, Debug, Clone)]
struct Config {
    /// List of peer servers, comes from the config file.
    servers: Vec<Peer>,
    /// Minimum number of milliseconds to wait for a heartbeat before triggering an election.
    election_timeout_min_ms: u32,
    /// Maximum number of milliseconds to wait for a heartbeat before triggering an election.
    election_timeout_max_ms: u32,
}

#[derive(Deserialize, Debug, Clone)]
struct Peer {
    /// unique identifier number for this server. 
    id: u64,
    /// Some string that can be parsed into a SocketAddr
    address: String,
    /// Some string that can be passed into tonic Channel builder
    uri: String,
}



#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let cli = Cli::parse();

    let config_file = std::fs::File::open("config.yaml")?;
    let config_arc:Arc<Config> = Arc::new(serde_yaml::from_reader(config_file)?);

    let my_config = config_arc.servers.iter().find(|s| s.id == cli.id).expect("couldn't find my config.");
    
    println!("MY config: {:?}", my_config);

    let addr = my_config.address.parse().unwrap();
    let scow_key_value = MyScowKeyValue::new();

    let server = Server::builder()
        .add_service(ScowKeyValueServer::new(scow_key_value))
        .serve(addr);    

        let election_handle = scow_key_value.election_loop_doer(config_arc, cli.id);

//    let _join_res = join!(server, election_handle);

//    println!("Join results: {:?}", _join_res);
    
    Ok(())
}

