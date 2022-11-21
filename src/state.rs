//! Server shared state
use crate::{
    client::{ActivityPubClient, Actor},
    util::host_from_uri,
    Error, Result,
};
use acidjson::AcidJson;
use axum::http::StatusCode;
use futures::future::try_join_all;
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, sync::Mutex};
use tracing::trace;

#[derive(Debug)]
pub struct State {
    pub db: Db,
    pub client: ActivityPubClient,
    object_cache: Mutex<HashMap<String, String>>,
}

impl State {
    pub fn new(db: Db, private_key_pem: &str) -> Self {
        Self {
            db,
            client: ActivityPubClient::new_with_priv_key(private_key_pem),
            object_cache: Default::default(),
        }
    }

    #[tracing::instrument(skip(self, message), err)]
    pub async fn post_for_actor<T: Serialize + Clone>(
        &self,
        actor: &Actor,
        object_id: String,
        cache_value: String,
        message: T,
    ) -> Result<()> {
        let inboxes = self.db.inboxes_for_actor(actor, &object_id)?;
        trace!(?inboxes, "posting message to all inboxes");

        // TODO: this will need to be smarter
        let res = try_join_all(
            inboxes
                .into_iter()
                .map(|inbox| self.client.json_post(inbox, message.clone())),
        )
        .await
        .map(|_| ());

        self.cache_object(object_id, cache_value);

        res
    }

    pub fn get_from_cache(&self, id: &str) -> Option<String> {
        self.object_cache.lock().unwrap().get(id).cloned()
    }

    pub fn cache_object(&self, object_id: String, activity_id: String) {
        self.object_cache
            .lock()
            .unwrap()
            .insert(object_id, activity_id);
    }
}

#[derive(Debug)]
pub struct Db {
    // map of host to inbox
    inboxes: AcidJson<HashMap<String, String>>,
}

impl Db {
    pub fn new(mut path: PathBuf) -> Result<Self> {
        if std::fs::create_dir_all(&path).is_err() {
            return Err(Error::StatusAndMessage {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "unable to create data dir",
            });
        }
        path.push("statedb.json");
        if std::fs::read(&path).is_err() && std::fs::write(&path, b"{}").is_err() {
            return Err(Error::StatusAndMessage {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "unable to create initial state db",
            });
        }

        match AcidJson::open(path.as_path()) {
            Ok(db) => Ok(Self { inboxes: db }),
            Err(_) => Err(Error::StatusAndMessage {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "unable to open state db",
            }),
        }
    }

    pub fn add_inbox_if_unknown(&self, inbox: String) -> Result<bool> {
        let host = host_from_uri(&inbox)?;

        if self.inboxes.read().contains_key(&inbox) {
            Ok(false)
        } else {
            self.inboxes.write().insert(host, inbox);
            Ok(true)
        }
    }

    pub fn remove_inbox(&self, inbox: &str) -> Result<String> {
        let host = host_from_uri(inbox)?;

        self.inboxes
            .write()
            .remove(&host)
            .ok_or(Error::StatusAndMessage {
                status: StatusCode::NOT_FOUND,
                message: "unknown inbox",
            })
    }

    pub fn inbox(&self, domain: &str) -> Option<String> {
        let domain = host_from_uri(domain).ok()?;

        self.inboxes.read().get(&domain).cloned()
    }

    pub fn inboxes_for_actor(&self, actor: &Actor, object_id: &str) -> Result<Vec<String>> {
        let origin_host = host_from_uri(object_id)?;

        let inboxes = self
            .inboxes
            .read()
            .iter()
            .filter(|&(host, inbox)| inbox != &actor.inbox && host != &origin_host)
            .map(|(_, inbox)| inbox.to_owned())
            .collect();

        Ok(inboxes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl State {
        pub fn new_with_test_key(db: Db) -> Self {
            Self {
                db,
                client: ActivityPubClient::new_with_test_key(),
                object_cache: Default::default(),
            }
        }
        pub fn clear(&self) {
            self.db.inboxes.write().clear();
        }
    }
}
