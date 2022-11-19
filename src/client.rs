//! A simple API client for making activitypub related requests
use crate::{base_url, signature::sign_request_headers, util::header_val, Error, Result};
use reqwest::{header, Client, Response, StatusCode};
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey, EncodeRsaPublicKey, LineEnding},
    pkcs1v15::SigningKey,
    RsaPrivateKey, RsaPublicKey,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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
}

impl ActivityPubClient {
    pub fn new_with_priv_key(priv_key_pem: &str) -> Self {
        let priv_key = RsaPrivateKey::from_pkcs1_pem(priv_key_pem)
            .expect("the provided private key for initialising the ActivityPubClient was invalid");
        let pub_key = RsaPublicKey::from(&priv_key);
        let signing_key = SigningKey::<Sha256>::new_with_prefix(priv_key);

        Self {
            signing_key,
            pub_key,
            client: Default::default(),
        }
    }

    pub fn pub_key(&self) -> String {
        self.pub_key
            .to_pkcs1_pem(LineEnding::default())
            .expect("to encode to PEM successfully")
    }

    async fn json_get<T: DeserializeOwned>(&self, uri: &str) -> Result<T> {
        let h = sign_request_headers(uri, None, &self.signing_key)?;
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
        let mut headers = sign_request_headers(uri, Some(&body), &self.signing_key)?;
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
        let Actor { id, inbox, .. } = self.get_actor(actor_uri).await?;
        info!(%id, %inbox, "sending follow request to inbox");

        let base = base_url();
        let message_id = Uuid::new_v4();
        let message = Activity {
            context: Context::default(),
            ty: ActivityType::Follow,
            to: vec![id.clone()],
            object: IdOrObject::Id(id),
            id: format!("https://{base}/activities/{message_id}"),
            actor: format!("https://{base}/actor)"),
        };

        self.json_post(inbox, message).await?;

        Ok(())
    }

    pub async fn unfollow_actor(&self, actor_uri: &str) -> Result<()> {
        let Actor { id, inbox, .. } = self.get_actor(actor_uri).await?;
        info!(%id, %inbox, "sending unfollow request to inbox");

        let base = base_url();
        let object_id = Uuid::new_v4();
        let message_id = Uuid::new_v4();
        let message = Activity {
            context: Context::default(),
            ty: ActivityType::Undo,
            to: vec![actor_uri.to_owned()],
            object: IdOrObject::Object {
                ty: ActivityType::Follow,
                object: actor_uri.to_owned(),
                actor: actor_uri.to_owned(),
                id: format!("https://{base}/activities/{object_id}"),
            },
            id: format!("https://{base}/activities/{message_id}"),
            actor: format!("https://{base}/actor)"),
        };

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

#[allow(dead_code)]
fn new_priv_key() -> RsaPrivateKey {
    RsaPrivateKey::new(&mut rand::thread_rng(), KEY_LEN).expect("failed to generate a key")
}

// NOTE: A subset of the rustypub Actor<'a> struct that can implement DeserializeOwned
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub id: String,
    pub inbox: String,
    #[serde(rename = "publicKey")]
    pub public_key_info: PublickKeyInfo,
}

impl Actor {
    pub fn key(&self) -> Result<RsaPublicKey> {
        RsaPublicKey::from_pkcs1_pem(&self.public_key_info.public_key_pem).map_err(|e| {
            Error::InvalidPublicKey {
                error: e.to_string(),
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublickKeyInfo {
    id: String,
    owner: String,
    public_key_pem: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityType {
    Accept,
    Announce,
    Create,
    Delete,
    Follow,
    Undo,
    Update,
}

// FIXME: according to the spec this can also be an object or an array of a string and (one or more?) object(s)?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Context(String);

impl Default for Context {
    fn default() -> Self {
        Self("https://www.w3.org/ns/activitystreams".to_owned())
    }
}

// Not a full implementation of an activitypub Activity: just enough for our purposes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    #[serde(rename = "@context")]
    pub context: Context,
    #[serde(rename = "type")]
    pub ty: ActivityType,
    pub to: Vec<String>,
    pub actor: String,
    pub object: IdOrObject,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IdOrObject {
    Id(String),

    Object {
        #[serde(rename = "type")]
        ty: ActivityType,
        id: String,
        actor: String,
        object: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::tests::{TEST_PRIV_KEY, TEST_PUB_KEY};
    use serde_json::json;

    impl ActivityPubClient {
        pub fn new_with_test_key() -> Self {
            Self::new_with_priv_key(TEST_PRIV_KEY)
        }
    }

    impl Actor {
        pub fn test_actor(id: &str) -> Self {
            Self {
                id: id.to_owned(),
                inbox: "https://example.com/inbox".to_owned(),
                public_key_info: PublickKeyInfo {
                    id: id.to_owned(),
                    owner: "self".to_owned(),
                    public_key_pem: TEST_PUB_KEY.to_owned(),
                },
            }
        }
    }

    const ID: &str = "foo";
    const MESSAGE_ID: &str = "https://example.com/activities/message_id";
    const ACTOR: &str = "https://example.com/actor";

    #[test]
    fn activity_serialises_with_id() {
        let raw = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "Follow",
            "to": [ID],
            "object": ID,
            "id": MESSAGE_ID,
            "actor": ACTOR,
        });

        let deserialized = serde_json::from_value::<Activity>(raw.clone());
        assert!(deserialized.is_ok());

        let serialized = serde_json::to_value(&deserialized.unwrap()).expect("to serialize");
        assert_eq!(serialized, raw);
    }

    #[test]
    fn activity_serialises_with_object() {
        let raw = json!({
            "@context": "https://www.w3.org/ns/activitystreams",
            "type": "Follow",
            "to": [ID],
            "object": {
              "type": "Follow",
              "object": ACTOR,
              "actor": ACTOR,
              "id": ID
            },
            "id": MESSAGE_ID,
            "actor": ACTOR,
        });

        let deserialized = serde_json::from_value::<Activity>(raw.clone());
        assert!(deserialized.is_ok());

        let serialized = serde_json::to_value(&deserialized.unwrap()).expect("to serialize");
        assert_eq!(serialized, raw);
    }
}
