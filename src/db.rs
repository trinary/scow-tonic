use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct DbDropGuard {
    db: Db,
}

impl DbDropGuard {
    pub(crate) fn db(&self) -> Db {
        self.db.clone()
    }

    pub fn new() -> Self {
        Self { 
            db: Db::new()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Db {
    shared: Arc<Shared>,
}

impl Db {
    pub(crate) fn new() -> Db {
        Db {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    entries: HashMap::new(),
                    current_term: 0,
                    role: Role::Follower,
                    voted_for: None,
                    last_heartbeat: Instant::now(),
                }),
            }),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<String> {
        let state = self.shared.state.lock().unwrap();
        state.entries.get(key).cloned()
    }

    pub(crate) fn set(&self, key: &str, value: &str) {
        let mut state = self.shared.state.lock().unwrap();
        let _prev = state.entries.insert(key.to_owned(), value.to_owned());
    }

    pub(crate) fn get_current_term(&self) -> u64 {
        let state = self.shared.state.lock().unwrap();
        state.current_term
    }

    pub(crate) fn set_current_term(&self, term: u64) {
        let mut state = self.shared.state.lock().unwrap();
        state.current_term = term
    }

    pub(crate) fn request_vote(&self, term: u64, candidate_id: u64, candidate_last_index: u64, candidate_last_term: u64) -> (u64, bool) {
        let mut state = self.shared.state.lock().unwrap();
        if term < state.current_term {
            (state.current_term, false)
        } else {
            if state.voted_for == None || state.voted_for == Some(candidate_id) {
                (state.current_term, true)
            } else {
                (state.current_term, false)
            }
        }
    }
}

#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
enum Role {
    #[default]
    Follower, 
    Leader, 
    Candidate
}

#[derive(Debug)]
struct State {
    entries: HashMap<String, String>,
    role: Role,
    current_term: u64,
    voted_for: Option<u64>,
    last_heartbeat: Instant,
}