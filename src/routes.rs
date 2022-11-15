//! Routes available on this server.
//!
//! We are implementing a subset of the activitypub API in order to function as a relay
use crate::{inbox, nodeinfo, well_known, State};
use axum::{
    routing::{get, post},
    Extension, Router,
};
use std::sync::Arc;

pub(crate) fn build_routes(state: Arc<State>) -> Router {
    Router::new()
        .route("/inbox", post(inbox::post))
        .route("/.well-known/webfinger", get(well_known::webfinger))
        .route("/.well-known/host-meta", get(well_known::host_meta))
        .route("/.well-known/nodeinfo", get(well_known::nodeinfo))
        .route("/nodeinfo/2.0", get(nodeinfo::get))
        .layer(Extension(state))
}
