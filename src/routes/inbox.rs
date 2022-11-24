use crate::{
    client::{Activity, ActivityType, Actor, IdOrObject},
    routes::extractors,
    signature::validate_signature,
    state::State,
    util::{host_from_uri, id_from_json},
    Error, Result,
};
use axum::{
    extract::{Extension, Host, Json, OriginalUri},
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
    OriginalUri(uri): OriginalUri,
    Extension(state): Extension<Arc<State>>,
    Json(req): Json<InboxRequest>,
) -> Result<extractors::Activity<Value>> {
    use ActivityType::*;

    let actor = state.client.get_actor(&req.actor).await?;

    validate_signature(&actor, "post", uri.path(), &headers)?;
    validate_request(&actor, req.ty, &state).await?;

    match req.ty {
        Announce | Create => handle_relay(&actor, req.activity, &host, state).await?,
        Delete | Update => handle_forward(&actor, req.activity, state).await?,
        Follow => handle_follow(&actor, req.activity, &host, state).await?,
        Undo => handle_undo(&actor, req.activity, state).await?,
        _ => (),
    };

    Ok(extractors::Activity(json!({})))
}

async fn validate_request(actor: &Actor, ty: ActivityType, state: &State) -> Result<()> {
    // TODO: reject the request based on config (block list, banned actors / software etc)

    let actor_domain = host_from_uri(&actor.id)?;
    if ty != ActivityType::Follow && state.db.inbox(&actor_domain).is_none() {
        info!(actor=%actor.id, "rejecting actor for trying to POST without following");
        return Err(Error::StatusAndMessage {
            status: StatusCode::UNAUTHORIZED,
            message: "access denied",
        });
    }

    Ok(())
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
        ..
    }: &Actor,
    activity: Value,
    host: &str,
    state: Arc<State>,
) -> Result<()> {
    if state.db.add_inbox_if_unknown(inbox.to_owned())? {
        // New inbox so follow the remote actor
        state.client.follow_actor(actor_id).await?;
    }

    let our_actor = format!("https://{}/actor", state.cfg.base_url());
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

#[cfg(test)]
mod validation_tests {
    use crate::state::Db;

    use super::*;
    use simple_test_case::test_case;

    #[tokio::test]
    async fn invalid_actor_uri_is_an_error() {
        let db = Db::new(std::env::temp_dir()).expect("unable to create database");
        let state = State::new_with_test_key(db);

        let actor = "example.com/without/scheme".to_owned();
        let res = validate_request(&Actor::test_actor(&actor), ActivityType::Create, &state).await;

        assert_eq!(res, Err(Error::InvalidUri { uri: actor }));

        state.clear();
    }

    #[test_case(ActivityType::Accept; "accept")]
    #[test_case(ActivityType::Announce; "announce")]
    #[test_case(ActivityType::Create; "create")]
    #[test_case(ActivityType::Delete; "delete")]
    #[test_case(ActivityType::Undo; "undo")]
    #[test_case(ActivityType::Update; "update")]
    #[tokio::test]
    async fn non_follow_for_unknown_inbox_is_an_error(ty: ActivityType) {
        let db = Db::new(std::env::temp_dir()).expect("unable to create database");
        let state = State::new_with_test_key(db);
        let res =
            validate_request(&Actor::test_actor("https://example.com/actor"), ty, &state).await;

        assert_eq!(
            res,
            Err(Error::StatusAndMessage {
                status: StatusCode::UNAUTHORIZED,
                message: "access denied"
            })
        );
        state.clear();
    }

    #[tokio::test]
    async fn follow_for_unknown_inbox_is_ok() {
        let db = Db::new(std::env::temp_dir()).expect("unable to create database");
        let state = State::new_with_test_key(db);
        let res = validate_request(
            &Actor::test_actor("https://example.com/actor"),
            ActivityType::Follow,
            &state,
        )
        .await;

        assert_eq!(res, Ok(()));
        state.clear();
    }

    #[test_case(ActivityType::Accept; "accept")]
    #[test_case(ActivityType::Announce; "announce")]
    #[test_case(ActivityType::Create; "create")]
    #[test_case(ActivityType::Delete; "delete")]
    #[test_case(ActivityType::Undo; "undo")]
    #[test_case(ActivityType::Update; "update")]
    #[tokio::test]
    async fn non_follow_for_known_inbox_is_ok(ty: ActivityType) {
        let db = Db::new(std::env::temp_dir()).expect("unable to create database");
        let state = State::new_with_test_key(db);
        state
            .db
            .add_inbox_if_unknown("https://example.com/actor".to_owned())
            .unwrap();

        let res =
            validate_request(&Actor::test_actor("https://example.com/actor"), ty, &state).await;

        assert_eq!(res, Ok(()));
        state.clear();
    }
}
