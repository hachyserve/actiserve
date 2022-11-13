use axum::{
    extract::Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum Error {
    #[error("Webfinger resource should begin with 'acct:', got '{resource}'")]
    MalformedWebfingerResource { resource: String },

    #[error("Webfinger uri should be of the form 'account@domain', got '{uri}'")]
    MalformedWebfingerUri { uri: String },

    #[error("Record not found: {id}")]
    StatusNotFound { id: String },
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        use Error::*;

        let error = self.to_string();

        let (status, data) = match self {
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

            StatusNotFound { id } => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": error, "id": id })),
            ),
        };

        (status, data).into_response()
    }
}
