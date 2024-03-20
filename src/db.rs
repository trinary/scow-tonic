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
        let state = self.shared.state.lock().unwrap();
        state.entries.get(key).cloned()
    }

    pub(crate) fn set(&self, key: &str, value: &str) {
        let mut state = self.shared.state.lock().unwrap();
        let _prev = state.entries.insert(key.to_owned(), value.to_owned());
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
