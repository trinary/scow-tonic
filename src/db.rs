use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct DbDropGuard {
    db: Db,
}

impl DbDropGuard {
    // pub(crate) fn new() -> DbDropGuard {
    //     DbDropGuard { db: Db::new() }
    // }

    pub(crate) fn db(&self) -> Db {
        self.db.clone()
    }
}

#[derive(Debug, Clone, Default)]
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
                    voted_for: None,
                }),
            }),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<String> {
        let state = self.shared.state.lock().unwrap();
        state.entries.get(key).map(|entry| entry.clone())
    }

    pub(crate) fn set(&self, key: String, value: String) {
        let mut state = self.shared.state.lock().unwrap();
        let _prev = state.entries.insert(key, value);
    }
}

#[derive(Debug, Default)]
struct Shared {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    entries: HashMap<String, String>,
    current_term: u64,
    voted_for: Option<u64>
}