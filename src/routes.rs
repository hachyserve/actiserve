//! Routes available on this server.
//!
//! We are implementing a subset of the activitypub API in order to function as a relay
use crate::{nodeinfo, statuses, State};
use axum::{
    routing::{delete, get, post},
    Extension, Router,
};

// TODO: remove statuses endpoints
pub(crate) fn build_routes(state: State) -> Router {
    Router::new()
        // .route("/inbox", post(inbox::post))
        // .route("/.well-known/webfinger", get(webfinger::get))
        .route("/.well-known/host-meta", get(well_known::host_meta))
        .route("/.well-known/nodeinfo", get(well_known::nodeinfo))
        .route("/nodeinfo/2.0", get(nodeinfo::get))
        .route("/api/v1/statuses", post(statuses::create))
        .route("/api/v1/statuses/:id", get(statuses::get))
        .route("/api/v1/statuses/:id", delete(statuses::delete))
        .layer(Extension(state))
}

pub mod well_known {
    use crate::{base_url, nodeinfo::NODE_INFO_SCHEMA};
    use axum::{extract::Json, http::header, response::IntoResponse};
    use serde_json::json;

    pub async fn nodeinfo() -> impl IntoResponse {
        let headers = [(header::CONTENT_TYPE, "application/json+jrd")];
        let base = base_url();
        let body = json!({
            "links": [
                {
                    "rel": NODE_INFO_SCHEMA,
                    "href": format!("{base}/nodeinfo/2.0"),
                }
            ]
        });

        (headers, Json(body))
    }

    pub async fn host_meta() -> impl IntoResponse {
        let headers = [(header::CONTENT_TYPE, "application/xrd+xml")];
        let base = base_url();
        let body = format!(
            r#"<?xml version="1.0"?>
<XRD xmlns="http://docs.oasis-open.org/ns/xri/xrd-1.0">
  <Link rel="lrdd" type="application/xrd+xml" template="{base}/.well-known/webfinger?resource={{uri}}"/>
</XRD>"#
        );

        (headers, body)
    }
}
