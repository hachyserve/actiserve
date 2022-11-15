//! A simple API client for making activitypub related requests
use crate::{base_url, Error, Result};
use reqwest::{header, Client, Response, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{error, info};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct ActivityPubClient {
    client: Client,
}

impl ActivityPubClient {
    async fn json_get<T: DeserializeOwned>(&self, uri: &str) -> Result<T> {
        match self.client.get(uri).send().await {
            Ok(raw) => raw.json().await.map_err(|e| Error::InvalidJson {
                uri: uri.to_owned(),
                raw: e.to_string(),
            }),

            Err(e) => Err(map_reqwest_error(uri, "GET", e)),
        }
    }

    pub async fn json_post(&self, uri: impl AsRef<str>, data: Value) -> Result<Response> {
        let body = serde_json::to_string(&data).map_err(|e| Error::InvalidJson {
            uri: uri.as_ref().to_owned(),
            raw: e.to_string(),
        })?;

        self.client
            .post(uri.as_ref())
            .body(body)
            .header(header::CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|e| map_reqwest_error(uri.as_ref(), "POST", e))
    }

    pub async fn get_actor(&self, uri: &str) -> Result<Actor> {
        match self.json_get(uri).await {
            Ok(actor) => Ok(actor),

            Err(Error::FailedRequest { status, .. }) if status == StatusCode::NOT_FOUND => {
                info!(%uri, "failed to fetch actor");
                Err(Error::StatusAndMessage {
                    status: StatusCode::NOT_FOUND,
                    message: "failed to fetch actor",
                })
            }

            Err(e) => {
                error!(error=%e, "failed to fetch actor");
                Err(e)
            }
        }
    }

    pub async fn unfollow_actor(&self, actor_uri: &str) -> Result<()> {
        let Actor { id, inbox } = self.get_actor(actor_uri).await?;
        info!(%id, %inbox, "sending unfollow request to inbox");

        let base = base_url();
        let object_id = Uuid::new_v4();
        let message_id = Uuid::new_v4();
        let message = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "Undo",
            "to": [actor_uri],
            "object": {
                "type": "Follow",
                "object": actor_uri,
                "actor": actor_uri,
                "id": format!("https://{base}/activities/{object_id}")
            },
            "id": format!("https://{base}/activities/{message_id}"),
            "actor": format!("https://{base}/actor)"),
        });

        self.json_post(inbox, message).await?;

        Ok(())
    }

    pub async fn follow_actor(&self, actor_uri: &str) -> Result<()> {
        let Actor { id, inbox } = self.get_actor(actor_uri).await?;
        info!(%id, %inbox, "sending follow request to inbox");

        let base = base_url();
        let message_id = Uuid::new_v4();
        let message = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "Follow",
            "to": [id],
            "object": id,
            "id": format!("https://{base}/activities/{message_id}"),
            "actor": format!("https://{base}/actor)"),
        });

        self.json_post(inbox, message).await?;

        Ok(())
    }
}

fn map_reqwest_error(uri: impl Into<String>, method: &str, e: reqwest::Error) -> Error {
    let status = e.status().unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let error = e.to_string();

    Error::FailedRequest {
        method: method.to_owned(),
        status,
        error,
        uri: uri.into(),
    }
}

// NOTE: A subset of the rustypub Actor<'a> struct that can implement DeserializeOwned
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub id: String,
    pub inbox: String,
}
