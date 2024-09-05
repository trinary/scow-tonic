use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
                }),
            }),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<String> {
        let state = self.shared.state.lock();
        match state {
            Ok(s) => s.entries.get(key).cloned(),
            Err(e) => {
                tracing::error!("Could not acquire lock on DB: {}", e);
                None
            }
        }
    }

    pub(crate) fn set(&self, key: &str, value: &str) {
        let state = self.shared.state.lock();
        match state {
            Ok(mut s) => {
                s.entries.insert(key.to_owned(), value.to_owned());
            }
            Err(e) => {
                tracing::error!("Could not acquire lock on DB for a SET: {}", e);
            }
        }
    }
}

#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    entries: HashMap<String, String>,
}
