use axum::{
    extract::Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("failed http request: {method} {uri} ({status}): {error}")]
    FailedRequest {
        method: String,
        status: StatusCode,
        error: String,
        uri: String,
    },

    #[error("invalid JSON from {uri}: {raw}")]
    InvalidJson { uri: String, raw: String },

    #[error("invalid public key pem: {error}")]
    InvalidPublicKey { error: String },

    #[error("'{uri}' is not a valid uri")]
    InvalidUri { uri: String },

    #[error("webfinger resource should begin with 'acct:', got '{resource}'")]
    MalformedWebfingerResource { resource: String },

    #[error("webfinger uri should be of the form 'account@domain', got '{uri}'")]
    MalformedWebfingerUri { uri: String },

    #[error("missing signature")]
    MissingSignature,

    #[error("{message}")]
    StatusAndMessage {
        status: StatusCode,
        message: &'static str,
    },
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        use Error::*;

        let error = self.to_string();

        let (status, data) = match self {
            FailedRequest {
                method,
                status,
                error,
                uri,
            } => (
                status,
                Json(json!({ "error": error, "uri": uri, "method": method })),
            ),

            InvalidJson { uri, raw } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": error, "uri": uri, "raw": raw })),
            ),

            InvalidPublicKey { .. } => (StatusCode::UNAUTHORIZED, Json(json!({ "error": error }))),

            InvalidUri { uri } => (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": error, "uri": uri })),
            ),

            MalformedWebfingerResource { resource } => (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": error,
                    "resource": resource,
                })),
            ),

            MalformedWebfingerUri { uri } => (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": error,
                    "uri": uri,
                })),
            ),

            MissingSignature => (StatusCode::UNAUTHORIZED, Json(json!({ "error": error }))),

            StatusAndMessage { status, message } => (status, Json(json!({ "error": message }))),
        };

        (status, data).into_response()
    }
}
