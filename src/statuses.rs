use crate::{Error, State};
use axum::{
    extract::{Json, Path},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::debug;
use uuid::Uuid;

#[cfg(test)]
use chrono::NaiveDateTime;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Status {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub in_reply_to: Option<String>,
    pub sensitive: bool,
    pub content: String,
    // TODO: more fields
}

impl Status {
    pub fn new(content: &str) -> Self {
        let id = Uuid::new_v4();

        #[cfg(not(test))]
        let now = Utc::now();
        #[cfg(test)]
        let now = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(42, 0), Utc);

        Self {
            id: id.to_string(),
            created_at: now,
            content: String::from(content),
            in_reply_to: None,
            sensitive: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
// TODO: figure out how to do "one-of" if not brute force
pub struct CreateStatusRequest {
    pub status: String,
    // TODO: media_ids
    // TODO: poll
    // TODO: other fields
}

impl CreateStatusRequest {
    fn to_status(&self) -> Status {
        Status::new(&self.status)
    }
}

pub async fn create(
    Json(req): Json<CreateStatusRequest>,
    Extension(state): Extension<State>,
) -> Response {
    let status = req.to_status();
    debug!(id = %status.id, "storing status");

    state
        .lock()
        .unwrap()
        .insert(status.id.clone(), status.clone());

    // TODO: send the status on to federated friends
    (StatusCode::CREATED, Json(status)).into_response()
}

pub async fn get(Path(id): Path<String>, Extension(state): Extension<State>) -> Response {
    debug!(%id, "getting status with ID");

    // TODO: unauthenticated error
    match state.lock().unwrap().get(&id) {
        Some(status) => Json(status.clone()).into_response(),
        None => Error::StatusNotFound { id }.into_response(),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteResponse {
    #[serde(flatten)]
    pub status: Status,
    pub text: String,
}

impl DeleteResponse {
    // TODO: polls and media
    pub fn new(status: &Status) -> Self {
        DeleteResponse {
            status: status.clone(),
            text: status.content.clone(),
        }
    }
}

impl std::ops::Deref for DeleteResponse {
    type Target = Status;

    fn deref(&self) -> &Self::Target {
        &self.status
    }
}

pub async fn delete(Path(id): Path<String>, Extension(state): Extension<State>) -> Response {
    debug!(%id, "deleting status");

    // TODO: propagate deletion to federated instances
    match state.lock().unwrap().remove(&id) {
        Some(status) => Json(DeleteResponse::new(&status)).into_response(),
        None => Error::StatusNotFound { id }.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_routes;
    use axum::{
        body::Body,
        http::{self, Request},
    };
    use serde::de::DeserializeOwned;
    use serde_json::json;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };
    use tower::ServiceExt; // for `app.oneshot()`

    const CONTENT: &str = "hello world";

    fn post_req(uri: &str, body: impl Serialize) -> Request<Body> {
        Request::builder()
            .method(http::Method::POST)
            .uri(uri)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    fn get_req(uri: &str) -> Request<Body> {
        Request::builder().uri(uri).body(Body::empty()).unwrap()
    }

    fn delete_req(uri: &str) -> Request<Body> {
        Request::builder()
            .method(http::Method::DELETE)
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    async fn from_body<T: DeserializeOwned>(resp: Response) -> T {
        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    // TODO: allow for inserting multiple statuses
    fn prepared_state() -> (String, State) {
        let s = Status::new(CONTENT);
        let id = s.id.clone();

        let mut state = HashMap::new();
        state.insert(id.clone(), s);

        (id, Arc::new(Mutex::new(state)))
    }

    #[tokio::test]
    async fn test_create_ok() {
        let app = build_routes(State::default());
        let req = post_req("/api/v1/statuses", json!({ "status": "hello world" }));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_get_ok() {
        let (id, state) = prepared_state();
        let app = build_routes(state);
        let req = get_req(&format!("/api/v1/statuses/{id}"));
        let resp = app.oneshot(req).await.unwrap();

        let status: Status = from_body(resp).await;

        assert_eq!(status.id, id);
        assert_eq!(status.content, CONTENT);
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let app = build_routes(Default::default());
        let req = get_req(&format!("/api/v1/statuses/{}", Uuid::nil()));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_ok() {
        let (id, state) = prepared_state();
        let app = build_routes(state);
        let req = delete_req(&format!("/api/v1/statuses/{id}"));
        let resp = app.oneshot(req).await.unwrap();

        let status: DeleteResponse = from_body(resp).await;

        assert_eq!(status.status.id, id);
        assert_eq!(status.status.content, CONTENT);
        assert_eq!(status.text, CONTENT);
    }

    #[tokio::test]
    async fn test_delete_not_found() {
        let app = build_routes(Default::default());
        let req = delete_req(&format!("/api/v1/statuses/{}", Uuid::nil()));
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
