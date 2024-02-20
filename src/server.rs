use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinSet;
use tokio::join;
use tonic::transport::Channel;
use tonic::transport::Server;
use clap::Parser;

use serde::Deserialize;

use scow::*;
use scow::scow_key_value_server::ScowKeyValueServer;

mod db;

mod scow_impl;
use crate::scow_key_value_client::ScowKeyValueClient;
use crate::scow_impl::MyScowKeyValue;


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
    servers: Vec<Peer>,
    vote_timeout_min_ms: u32,
    vote_timeout_max_ms: u32,
}

#[derive(Deserialize, Debug, Clone)]
struct Peer {
    id: u64,
    address: String,
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


    let heartbeat = tokio::spawn(
        heartbeat(config_arc, cli.id)
    );

    let server = Server::builder()
        .add_service(ScowKeyValueServer::new(scow_key_value))
        .serve(addr);

    join!(server, heartbeat);
    
    Ok(())
}

async fn build_client(peer: &Peer) -> Result<ScowKeyValueClient<Channel>, Box<dyn std::error::Error>> {
    if let Ok(uri) = peer.uri.parse() {
        let endpoint = tonic::transport::channel::Channel::builder(uri);
        Ok(ScowKeyValueClient::new(endpoint.connect_lazy()))
    } else {
        Err("invalid uri")?
    }
}

async fn initiate_vote(peer_clients: Vec<ScowKeyValueClient<Channel>>) -> Vec<RequestVoteReply> {
    println!("initiated vote from heartbeat???");
    let mut set = JoinSet::new();
    let mut replies = vec![];

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

async fn heartbeat(config_arc: Arc<Config>, my_id: u64) -> () {
    let mut peer_clients: Vec<ScowKeyValueClient<Channel>> = vec![];
    let peer_configs: Vec<&Peer> = config_arc.servers.iter().filter(|s| s.id != my_id).collect();

    for p in peer_configs.iter() {
        let client = build_client(p).await;
        match client {
            Ok(c) => peer_clients.push(c),
            Err(_) => panic!("asdfasdfd"),
        }
    }
    println!("peer clients?!!? {:?}", peer_clients);

    let mut interval = tokio::time::interval(Duration::from_millis(500));
    println!("gonna start heartbeating.");
    loop {
        println!("heartbeat loop inner");
        interval.tick().await;
        let vote_res = initiate_vote(peer_clients.clone()).await;
        println!("vote results:{:?}", vote_res);
    }
}