//! Helpers for setting the correct content type when building responses
use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

/// A helper for returning a JSON jrd document with the correct content header
#[derive(Debug, Serialize, Deserialize)]
pub struct Jrd<T>(pub T);

impl<T> IntoResponse for Jrd<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        match serde_json::to_string(&self.0) {
            Ok(s) => ([(header::CONTENT_TYPE, "application/jrd+json")], s).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain;charset=UTF-8")],
                err.to_string(),
            )
                .into_response(),
        }
    }
}

/// A helper for returning an activitypub JSON document with the correct content header
#[derive(Debug, Serialize, Deserialize)]
pub struct Activity<T>(pub T);

impl<T> IntoResponse for Activity<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        match serde_json::to_string(&self.0) {
            Ok(s) => ([(header::CONTENT_TYPE, "application/activity+json")], s).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain;charset=UTF-8")],
                err.to_string(),
            )
                .into_response(),
        }
    }
}
