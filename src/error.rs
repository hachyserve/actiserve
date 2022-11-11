use axum::{
    extract::Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum Error {
    #[error("Record not found: {id}")]
    StatusNotFound { id: String },
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        use Error::*;

        let (status, data) = match self {
            StatusNotFound { .. } => (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "Record not found"})),
            ),
        };

        (status, data).into_response()
    }
}
