//! Server shared state
use crate::statuses::Status;
use std::{collections::HashMap, sync::Mutex};

#[derive(Debug, Default)]
pub struct State {
    pub config: Config,
    pub db: Db,
}

#[derive(Debug, Default, Clone)]
pub struct Config {}

// TODO: persistent store for the statuses
#[derive(Debug, Default)]
pub struct Db {
    statuses: Mutex<HashMap<String, Status>>,
}

impl Db {
    pub fn insert(&self, id: String, status: Status) {
        self.statuses.lock().unwrap().insert(id, status);
    }

    pub fn get(&self, id: &str) -> Option<Status> {
        self.statuses.lock().unwrap().get(id).cloned()
    }

    pub fn remove(&self, id: &str) -> Option<Status> {
        self.statuses.lock().unwrap().remove(id)
    }
}
