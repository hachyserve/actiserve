//! Routes available on this server.
//!
//! We are implementing a subset of the activitypub API in order to function as a relay
use crate::{nodeinfo, statuses, well_known, State};
use axum::{
    routing::{delete, get, post},
    Extension, Router,
};

// TODO: remove statuses endpoints
pub(crate) fn build_routes(state: State) -> Router {
    Router::new()
        // .route("/inbox", post(inbox::post))
        .route("/.well-known/webfinger", get(well_known::webfinger))
        .route("/.well-known/host-meta", get(well_known::host_meta))
        .route("/.well-known/nodeinfo", get(well_known::nodeinfo))
        .route("/nodeinfo/2.0", get(nodeinfo::get))
        .route("/api/v1/statuses", post(statuses::create))
        .route("/api/v1/statuses/:id", get(statuses::get))
        .route("/api/v1/statuses/:id", delete(statuses::delete))
        .layer(Extension(state))
}
