//! Routes available on this server.
//!
//! We are implementing a subset of the activitypub API in order to function as a relay

use crate::state::State;

use axum::{
    extract::Host,
    routing::{get, post},
    Extension, Router,
};
use rustypub::core::ContextBuilder;
use serde_json::{json, Value};
use std::sync::Arc;

mod extractors;
mod inbox;
mod nodeinfo;
mod well_known;

pub fn build_routes(state: Arc<State>) -> Router {
    Router::new()
        .route("/actor", get(get_actor))
        .route("/inbox", post(inbox::post))
        .route("/.well-known/webfinger", get(well_known::webfinger))
        .route("/.well-known/host-meta", get(well_known::host_meta))
        .route("/.well-known/nodeinfo", get(well_known::nodeinfo))
        .route("/nodeinfo/2.0", get(nodeinfo::get))
        .layer(Extension(state))
}

pub async fn get_actor(
    Host(host): Host,
    Extension(state): Extension<Arc<State>>,
) -> extractors::Activity<Value> {
    extractors::Activity(json!({
        "@context": ContextBuilder::default().build(),
        "endpoints": {
            "sharedInbox": format!("https://{host}/inbox"),
        },
        "followers": format!("https://{host}/followers"),
        "following": format!("https://{host}/following"),
        "inbox": format!("https://{host}/inbox"),
        "name": "Actiserve",
        "type": "Application",
        "id": format!("https://{host}/actor"),
        "publicKey": {
            "id": format!("https://{host}/actor#main-key"),
            "owner": format!("https://{host}/actor"),
            "publicKeyPem": state.client.pub_key(),
        },
        "summary": "Actiserve bot",
        "preferredUsername": "relay",
        "url": format!("https://{host}/actor"),
    }))
}
