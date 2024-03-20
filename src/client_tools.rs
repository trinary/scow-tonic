use std::time::Duration;

use tonic::transport::Channel;

use crate::{scow_key_value_client::ScowKeyValueClient, Peer};


pub fn build_client(peer: &Peer) -> Result<ScowKeyValueClient<Channel>, Box<dyn std::error::Error>> {
    if let Ok(uri) = peer.uri.parse() {
        let endpoint = tonic::transport::channel::Channel::builder(uri).connect_timeout(Duration::from_millis(100));
        Ok(ScowKeyValueClient::new(endpoint.connect_lazy()))
    } else {
        Err("invalid uri")?
    }
}