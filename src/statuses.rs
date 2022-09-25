use std::collections::HashMap;
use std::sync::Mutex;

use actix_web::web::{Json, Path};
use actix_web::HttpResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::constants::APPLICATION_JSON;

lazy_static! {
    static ref STATUS_DB: Mutex<HashMap<String, Status>> = {
        let m = HashMap::new();
        Mutex::new(m)
    };
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Error {
    pub error: String,
}

impl Error {
    fn new(error: &str) -> Self {
        Error {
            error: error.to_string(),
        }
    }
}

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
        Self {
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
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

#[post("/api/v1/statuses")]
pub async fn create(req: Json<CreateStatusRequest>) -> HttpResponse {
    let status = req.to_status();
    // TODO: persistent store for the statuses
    STATUS_DB
        .lock()
        .unwrap()
        .insert(status.id.clone(), status.clone());
    // TODO: send the status on to federated friends
    HttpResponse::Created()
        .content_type(APPLICATION_JSON)
        .json(status)
}

#[get("/api/v1/statuses/{id}")]
pub async fn get(path: Path<String>) -> HttpResponse {
    let id = path.into_inner();
    // TODO: unauthenticated error
    match STATUS_DB.lock().unwrap().get(&id) {
        Some(status) => HttpResponse::Ok()
            .content_type(APPLICATION_JSON)
            .json(status),
        None => HttpResponse::NotFound()
            .content_type(APPLICATION_JSON)
            .json(Error::new("Record not found")),
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

#[delete("/api/v1/statuses/{id}")]
pub async fn delete(path: Path<String>) -> HttpResponse {
    let id = path.into_inner();
    // TODO: propogate deletion to federated instances
    match STATUS_DB.lock().unwrap().remove(&id) {
        Some(status) => HttpResponse::Ok()
            .content_type(APPLICATION_JSON)
            .json(DeleteResponse::new(&status)),
        None => HttpResponse::NotFound()
            .content_type(APPLICATION_JSON)
            .json(Error::new("Record not found")),
    }
}
