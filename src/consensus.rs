use std::default;
use std::net::SocketAddr;
use std::fmt::Display;

#[derive(Debug, PartialEq)]
pub enum ServerState {
    Leader,
    Follower,
}

impl Default for ServerState {
    fn default() -> Self {
        Self::Follower
    }
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub struct ServerId {
    pub id: u32,
    pub address: SocketAddr,
}

impl Display for ServerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({} {})", self.id, self.address)
    }
}

#[derive(Debug)]
pub struct TermState {
    pub current_term: u64,
    pub server_id: ServerId,
    pub server_state: ServerState,
    pub leader: Option<ServerId>,
}

impl TermState {
    pub fn new() -> TermState {
        Default::default()
    }
}

impl Default for TermState {
    fn default() -> Self {
        Self {
            current_term: 0,
            server_id: ServerId {
                id: Default::default(),
                address: "[::1]:50051".parse().unwrap(),
            },
            server_state: ServerState::Follower,
            leader: None
        }
    }
}
