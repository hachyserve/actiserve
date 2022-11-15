//! Utility functions
use crate::{Error, Result};
use axum::http::Uri;
use serde_json::Value;

pub fn host_from_uri(uri: &str) -> Result<String> {
    let parsed = uri.parse::<Uri>().map_err(|_| Error::InvalidUri {
        uri: uri.to_owned(),
    })?;

    let host = parsed.host().ok_or_else(|| Error::InvalidUri {
        uri: uri.to_owned(),
    })?;

    Ok(host.to_owned())
}

pub fn id_from_json(val: &Value) -> String {
    let obj = &val["object"];

    let id = match obj.get("id") {
        Some(id) => id.as_str(),
        None => obj.as_str(),
    };

    id.unwrap().to_owned()
}
