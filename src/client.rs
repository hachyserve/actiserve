//! A simple API client for making activitypub related requests
use crate::{signature::sign_request_headers, util::header_val, Error, Result};
use reqwest::{header, Client, Response, StatusCode};
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, EncodeRsaPublicKey, LineEnding},
    pkcs1v15::SigningKey,
    RsaPrivateKey, RsaPublicKey,
};
use rustypub::{
    core::{ActivityBuilder, ObjectBuilder},
    extended::{Actor, ActorBuilder},
};
use serde::{de::DeserializeOwned, Serialize};
use sha2::Sha256;
use tracing::{error, info};
use uuid::Uuid;

const KEY_LEN: usize = 1024;
// const KEY_LEN: usize = 4096;

#[derive(Debug)]
pub struct ActivityPubClient {
    signing_key: SigningKey<Sha256>,
    pub_key: RsaPublicKey,
    client: Client,
    base: String,
}

impl ActivityPubClient {
    pub fn new_with_priv_key(priv_key_pem: &str, base: String) -> Self {
        let priv_key = RsaPrivateKey::from_pkcs1_pem(priv_key_pem)
            .expect("the provided private key for initialising the ActivityPubClient was invalid");
        let pub_key = RsaPublicKey::from(&priv_key);
        let signing_key = SigningKey::<Sha256>::new_with_prefix(priv_key);

        Self {
            signing_key,
            pub_key,
            client: Default::default(),
            base,
        }
    }

    pub fn pub_key(&self) -> String {
        self.pub_key
            .to_pkcs1_pem(LineEnding::default())
            .expect("to encode to PEM successfully")
    }

    async fn json_get<T: DeserializeOwned>(&self, uri: &str) -> Result<T> {
        let h = sign_request_headers(&self.base, uri, None, &self.signing_key)?;
        match self.client.get(uri).headers(h).send().await {
            Ok(raw) => raw.json().await.map_err(|e| Error::InvalidJson {
                uri: uri.to_owned(),
                raw: e.to_string(),
            }),

            Err(e) => Err(map_reqwest_error(uri, "GET", e)),
        }
    }

    pub async fn json_post<T: Serialize>(&self, uri: impl AsRef<str>, data: T) -> Result<Response> {
        let body = serde_json::to_string(&data).map_err(|e| Error::InvalidJson {
            uri: uri.as_ref().to_owned(),
            raw: e.to_string(),
        })?;

        let uri = uri.as_ref();
        let mut headers = sign_request_headers(&self.base, uri, Some(&body), &self.signing_key)?;
        headers.insert(
            header::CONTENT_TYPE,
            header_val("application/activity+json")?,
        );

        self.client
            .post(uri)
            .body(body)
            .headers(headers)
            .send()
            .await
            .map_err(|e| map_reqwest_error(uri, "POST", e))
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

    pub async fn follow_actor(&self, actor_uri: &str) -> Result<()> {
        let base = &self.base;
        let actor: Actor = self.get_actor(actor_uri).await?;
        let actor_id = actor.id.as_ref().ok_or(Error::StatusAndMessage {
            status: StatusCode::BAD_REQUEST,
            message: "actor has no id",
        })?;
        let id = actor_id
            .parse::<http::Uri>()
            .map_err(|_e| Error::InvalidUri {
                uri: actor_id.clone(),
            })?;
        info!(
            "sending follow request to inbox: {:?} {:?}",
            id, actor.inbox
        );

        let message_id = Uuid::new_v4();
        let message_id_uri = format!("https://{base}/activities/{message_id}");
        let actor_uri = format!("https://{base}/actor");
        let message = ActivityBuilder::new(String::from("Follow"), String::from("Following actor"))
            .actor(
                ActorBuilder::new(String::from("Actor")).url(
                    actor_uri
                        .parse::<http::Uri>()
                        .map_err(|_e| Error::InvalidUri { uri: actor_uri })?,
                ),
            )
            .to(vec![actor.id.as_ref().expect("actor has no id").clone()])
            .object(ObjectBuilder::new().id(id))
            .id(message_id_uri
                .parse::<http::Uri>()
                .map_err(|_e| Error::InvalidUri {
                    uri: message_id_uri,
                })?)
            .build();

        self.json_post(actor.inbox.expect("actor has no inbox"), message)
            .await?;

        Ok(())
    }

    pub async fn unfollow_actor(&self, actor_uri: &str) -> Result<()> {
        let base = &self.base;
        let actor: Actor = self.get_actor(actor_uri).await?;
        info!(
            "sending unfollow request to inbox: {:?} {:?}",
            actor.id, actor.inbox
        );

        let object_id = Uuid::new_v4();
        let message_id = Uuid::new_v4();
        let activity_id = format!("https://{base}/activities/{message_id}");
        let activity_id_uri = activity_id
            .parse::<http::Uri>()
            .map_err(|_e| Error::InvalidUri { uri: activity_id })?;
        let object_id = format!("https://{base}/activities/{object_id}");
        let object_id_uri = object_id
            .parse::<http::Uri>()
            .map_err(|_e| Error::InvalidUri { uri: object_id })?;

        let message = ActivityBuilder::new(String::from("Undo"), String::from("Unfollow actor"))
            .actor(
                ActorBuilder::new(String::from("Actor")).url(
                    format!("https://{base}/actor")
                        .parse::<http::Uri>()
                        .map_err(|_e| Error::InvalidUri {
                            uri: format!("https://{base}/actor"),
                        })?,
                ),
            )
            .to(vec![actor_uri.to_owned()])
            .object(
                // FIXME: object does not have a property "actor":
                // https://www.w3.org/TR/activitystreams-vocabulary/#types
                // .actor
                // object does not have a property "object":
                // https://www.w3.org/TR/activitystreams-vocabulary/#types
                // .object
                ObjectBuilder::new()
                    .object_type(String::from("Follow"))
                    .id(object_id_uri),
            )
            .id(activity_id_uri)
            .build();

        self.json_post(actor.inbox.expect("actor has no inbox"), message)
            .await?;

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

#[allow(dead_code)]
fn new_priv_key() -> RsaPrivateKey {
    RsaPrivateKey::new(&mut rand::thread_rng(), KEY_LEN).expect("failed to generate a key")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::tests::TEST_PRIV_KEY;

    impl ActivityPubClient {
        pub fn new_with_test_key() -> Self {
            Self::new_with_priv_key(TEST_PRIV_KEY, "127.0.0.1:4242".to_string())
        }
    }
}
