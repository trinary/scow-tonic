use clap::Parser;
use scow::scow_key_value_server::ScowKeyValueServer;
use scow::*;
use serde::Deserialize;
use std::sync::Arc;
use tokio::join;
use tokio::sync::mpsc;
use tonic::transport::Server;

use crate::election::ElectionHandler;
use crate::heartbeat::Heartbeat;
use crate::scow_impl::MyScowKeyValue;
use crate::state_handler::StateHandler;

mod db;
mod election;
mod heartbeat;
mod scow_impl;
mod state_handler;

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
    /// Number of milliseconds between each heartbeat from the leader
    heartbeat_interval_ms: u32,
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
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    //    console_subscriber::init();

    let cli = Cli::parse();

    let config_file = std::fs::File::open("config.yaml")?;
    let config_arc: Arc<Config> = Arc::new(serde_yaml::from_reader(config_file)?);

    let my_config = config_arc
        .servers
        .iter()
        .find(|s| s.id == cli.id)
        .expect("couldn't find my config.");

    tracing::info!("MY config: {:?}", my_config);

    let (state_tx, mut state_rx) = mpsc::channel(32);

    let mut state_handler = StateHandler::new(state_rx, &config_arc.servers, cli.id);

    tokio::spawn(async move {
        let _ = state_handler.run().await;
    });

    let addr = my_config.address.parse().unwrap();
    let scow_key_value = MyScowKeyValue::new(state_tx.clone());

    let election_handler = ElectionHandler::new(state_tx.clone(), config_arc.clone());
    let election_future = election_handler.run_election_loop();

    let heartbeat_handler = Heartbeat::new(state_tx.clone(), config_arc.clone());
    let heartbeat_future = heartbeat_handler.run_heartbeat_loop();

    let server = Server::builder()
        .add_service(ScowKeyValueServer::new(scow_key_value))
        .serve(addr);

    let _join_res = join!(server, heartbeat_future, election_future);

    Ok(())
}
