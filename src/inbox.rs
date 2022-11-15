use crate::{
    base_url,
    client::{Activity, ActivityType, Actor, IdOrObject},
    extractors,
    util::{host_from_uri, id_from_json},
    Error, Result, State,
};
use axum::{
    extract::{Extension, Host, Json},
    http::{header::HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct InboxRequest {
    #[serde(rename = "type")]
    ty: ActivityType,
    actor: String,
    activity: Value,
}

#[tracing::instrument(level = "debug", fields(host, headers), err)]
pub async fn post(
    headers: HeaderMap,
    Host(host): Host,
    Extension(state): Extension<Arc<State>>,
    Json(req): Json<InboxRequest>,
) -> Result<extractors::Activity<Value>> {
    if !headers.contains_key("signature") {
        return Err(Error::MissingSignature);
    }

    let actor = state.client.get_actor(&req.actor).await?;

    // TODO: reject the request based on config (block list, banned actors / software etc)

    let actor_domain = host_from_uri(&req.actor)?;
    if req.ty != ActivityType::Follow && state.db.inbox(&actor_domain).is_none() {
        info!(actor=%req.actor, "rejecting actor for trying to POST without following");
        return Err(Error::StatusAndMessage {
            status: StatusCode::UNAUTHORIZED,
            message: "access denied",
        });
    }

    use ActivityType::*;

    match req.ty {
        Announce | Create => handle_relay(&actor, req.activity, &host, state).await?,
        Delete | Update => handle_forward(&actor, req.activity, state).await?,
        Follow => handle_follow(&actor, req.activity, &host, state).await?,
        Undo => handle_undo(&actor, req.activity, state).await?,
        _ => (),
    };

    Ok(extractors::Activity(json!({})))
}

#[tracing::instrument(level = "info", skip(state, activity), err)]
async fn handle_relay(actor: &Actor, activity: Value, host: &str, state: Arc<State>) -> Result<()> {
    let object_id = id_from_json(&activity);

    if let Some(activity_id) = state.get_from_cache(&object_id) {
        info!(%object_id, %activity_id, "ID has already been relayed");
        return Ok(());
    }

    info!(id=%actor.id, "relaying post from actor");
    let activity_id = format!("https://{host}/activities/{}", Uuid::new_v4());
    let message = Activity {
        context: Default::default(),
        ty: ActivityType::Announce,
        to: vec![format!("https://{host}/followers")],
        object: IdOrObject::Id(object_id.clone()),
        id: activity_id.clone(),
        actor: format!("https://{host}/actor)"),
    };

    debug!(?message, "relaying message");
    state
        .post_for_actor(actor, object_id, activity_id, message)
        .await
}

#[tracing::instrument(level = "info", skip(state, activity), err)]
async fn handle_forward(actor: &Actor, activity: Value, state: Arc<State>) -> Result<()> {
    let object_id = id_from_json(&activity);

    if state.get_from_cache(&object_id).is_some() {
        info!(%object_id, "already forwarded");
        return Ok(());
    }

    info!(%actor.id, "forwarding post");
    state
        .post_for_actor(actor, object_id.clone(), object_id, activity)
        .await
}

#[tracing::instrument(level = "info", skip(state, activity), err)]
async fn handle_follow(
    Actor {
        id: actor_id,
        inbox,
    }: &Actor,
    activity: Value,
    host: &str,
    state: Arc<State>,
) -> Result<()> {
    if state.db.add_inbox_if_unknown(inbox.to_owned())? {
        // New inbox so follow the remote actor
        state.client.follow_actor(actor_id).await?;
    }

    let our_actor = format!("https://{}/actor", base_url());
    let object_id = id_from_json(&activity);
    let message_id = Uuid::new_v4();

    let message = Activity {
        context: Default::default(),
        ty: ActivityType::Accept,
        to: vec![actor_id.clone()],
        object: IdOrObject::Object {
            ty: ActivityType::Follow,
            id: object_id,
            object: our_actor.clone(),
            actor: actor_id.clone(),
        },
        actor: our_actor,
        id: format!("https://{host}/activities/{message_id}"),
    };

    state.client.json_post(inbox, message).await?;

    Ok(())
}

#[tracing::instrument(level = "info", skip(state, activity), err)]
async fn handle_undo(actor: &Actor, activity: Value, state: Arc<State>) -> Result<()> {
    let ty = match activity["object"]["type"].as_str() {
        Some(ty) => ty.to_owned(),
        None => {
            return Err(Error::StatusAndMessage {
                status: StatusCode::BAD_REQUEST,
                message: "no object type",
            })
        }
    };

    match ty.as_ref() {
        "Follow" => {
            state.db.remove_inbox(&actor.id)?;
            state.client.unfollow_actor(&actor.id).await
        }

        "Announce" => handle_forward(actor, activity, state).await,

        _ => Ok(()),
    }
}
