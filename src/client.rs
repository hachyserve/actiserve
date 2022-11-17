//! A simple API client for making activitypub related requests
use crate::{base_url, Error, Result};
use axum::http::{HeaderMap, HeaderValue, Uri};
use chrono::Utc;
use reqwest::{header, Client, Response, StatusCode};
use rsa::{
    pkcs1::{EncodeRsaPublicKey, LineEnding},
    PaddingScheme, PublicKey, RsaPrivateKey, RsaPublicKey,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

const KEY_LEN: usize = 1024;
// const KEY_LEN: usize = 4096;

#[derive(Debug)]
pub struct ActivityPubClient {
    #[allow(dead_code)]
    priv_key: RsaPrivateKey,
    pub_key: RsaPublicKey,
    client: Client,
}

impl Default for ActivityPubClient {
    fn default() -> Self {
        Self::new_with_key(new_priv_key())
    }
}

impl ActivityPubClient {
    pub fn new_with_key(priv_key: RsaPrivateKey) -> Self {
        let pub_key = RsaPublicKey::from(&priv_key);

        Self {
            priv_key,
            pub_key,
            client: Default::default(),
        }
    }

    pub fn pub_key(&self) -> String {
        self.pub_key
            .to_pkcs1_pem(LineEnding::default())
            .expect("to encode to PEM successfully")
    }

    fn signed_request_headers(&self, uri: &str, data: Option<&str>) -> Result<HeaderMap> {
        let uri = uri.parse::<Uri>().map_err(|_| Error::InvalidUri {
            uri: uri.to_owned(),
        })?;

        let method = if data.is_some() { "POST" } else { "GET" };
        let path = uri.path();
        let host = uri.host().ok_or(Error::InvalidUri {
            uri: uri.to_string(),
        })?;

        let mut headers = HeaderMap::new();
        headers.insert("(request-target)", header_val(&format!("{method} {path}"))?);
        headers.insert("Date", header_val(&Utc::now().to_string())?);
        headers.insert("Host", header_val(host)?);

        if let Some(s) = data {
            headers.insert("Content-Length", header_val(&s.len().to_string())?);

            let h = hmac_sha256::Hash::hash(s.as_bytes());
            let digest = base64::encode(h);
            headers.insert("Digest", header_val(&format!("SHA-256={digest}"))?);
        }

        let signature = create_signature(&headers, &self.pub_key);
        headers.insert("Signature", header_val(&signature)?);

        // Now that we've generated the signature we can remove what we no longer need
        headers.remove("(request-target)");
        headers.remove("Host");

        Ok(headers)
    }

    async fn json_get<T: DeserializeOwned>(&self, uri: &str) -> Result<T> {
        let h = self.signed_request_headers(uri, None)?;
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
        let mut headers = self.signed_request_headers(uri, Some(&body))?;
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

    pub async fn unfollow_actor(&self, actor_uri: &str) -> Result<()> {
        let Actor { id, inbox } = self.get_actor(actor_uri).await?;
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

    pub async fn follow_actor(&self, actor_uri: &str) -> Result<()> {
        let Actor { id, inbox } = self.get_actor(actor_uri).await?;
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

fn new_priv_key() -> RsaPrivateKey {
    RsaPrivateKey::new(&mut rand::thread_rng(), KEY_LEN).expect("failed to generate a key")
}

// We should never be trying to construct an invalid header value in sign_request
// below so if this pops we've definitely messed up somewhere
fn header_val(s: &str) -> Result<HeaderValue> {
    HeaderValue::from_str(s).map_err(|_| Error::StatusAndMessage {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "internal server error",
    })
}

fn create_signature(headers: &HeaderMap, pub_key: &RsaPublicKey) -> String {
    // Converting to a vec of pairs to ensure the iteration order is consistent in
    // both the signature and the list of used headers
    let pairs: Vec<_> = headers
        .iter()
        .map(|(k, v)| {
            (
                k.to_string().to_lowercase(),
                v.to_str().expect("valid ascii header values"),
            )
        })
        .collect();

    let sig_string: String = pairs
        .iter()
        .map(|(k, v)| format!("{k}: \"{v}\"",))
        .collect::<Vec<String>>()
        .join("\n");

    let signed_bytes = pub_key
        .encrypt(
            &mut rand::thread_rng(),
            PaddingScheme::new_pkcs1v15_encrypt(),
            sig_string.as_bytes(),
        )
        .expect("encryption to succeed");

    let signature = String::from_utf8(signed_bytes).expect("valid utf8 from encrypting");

    let used_headers = pairs
        .into_iter()
        .map(|(k, _)| k)
        .collect::<Vec<_>>()
        .join(" ");

    vec![
        format!("keyId=\"https://{}/actor#main-key\"", base_url()),
        "algorithm=\"rsa-sha256\"".to_owned(),
        format!("headers=\"{used_headers}\""),
        format!("signature=\"{signature}\""),
    ]
    .join(",")
}

// NOTE: A subset of the rustypub Actor<'a> struct that can implement DeserializeOwned
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub id: String,
    pub inbox: String,
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
    use serde_json::json;

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
