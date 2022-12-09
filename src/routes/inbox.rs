use crate::{
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
use rustypub::{
    core::{ActivityBuilder, ObjectBuilder},
    extended::{Actor, ActorBuilder},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct InboxRequest {
    #[serde(rename = "type")]
    ty: String,
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
    let actor = state.client.get_actor(&req.actor).await?;

    validate_signature(&actor, "post", uri.path(), &headers)?;
    validate_request(&actor, &req.ty, &state).await?;

    match &*req.ty {
        "Announce" | "Create" => handle_relay(&actor, req.activity, &host, state).await?,
        "Delete" | "Update" => handle_forward(&actor, req.activity, state).await?,
        "Follow" => handle_follow(&actor, req.activity, &host, state).await?,
        "Undo" => handle_undo(&actor, req.activity, state).await?,
        _ => (),
    };

    Ok(extractors::Activity(json!({})))
}
async fn validate_request(actor: &Actor, ty: &str, state: &State) -> Result<()> {
    // TODO: reject the request based on config (block list, banned actors / software etc)
    let actor_id = actor.id.as_ref().expect("actor has no id");

    let actor_domain = host_from_uri(actor_id)?;
    if ty != "Follow" && state.db.inbox(&actor_domain).is_none() {
        info!(actor=%actor_id, "rejecting actor for trying to POST without following");
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
    let object_id_uri = &object_id
        .parse::<http::Uri>()
        .map_err(|_e| Error::InvalidUri {
            uri: object_id.clone(),
        })?;
    let actor_id = actor.id.as_ref().expect("actor has no id");

    if let Some(activity_id) = state.get_from_cache(&object_id) {
        info!(%object_id, %activity_id, "ID has already been relayed");
        return Ok(());
    }

    info!(id=%actor_id, "relaying post from actor");
    let activity_id = format!("https://{host}/activities/{}", Uuid::new_v4());
    let activity_id_uri = &activity_id
        .parse::<http::Uri>()
        .map_err(|_e| Error::InvalidUri {
            uri: activity_id.clone(),
        })?;

    let actor_uri = format!("https://{host}/actor")
        .parse::<http::Uri>()
        .map_err(|_e| Error::InvalidUri {
            uri: format!("https://{host}/actor"),
        })?;

    let message = ActivityBuilder::new(
        String::from("Announce"),
        String::from("announcing post from actor"),
    )
    .to(vec![format!("https://{host}/followers")])
    .id(activity_id_uri.clone())
    .actor(ActorBuilder::new(String::from("Actor")).url(actor_uri))
    .object(ObjectBuilder::new().id(object_id_uri.clone()))
    .build();

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

    let actor_id = actor.id.as_ref().ok_or(Error::StatusAndMessage {
        status: StatusCode::BAD_REQUEST,
        message: "actor has no id",
    })?;

    info!(%actor_id, "forwarding post");
    state
        .post_for_actor(actor, object_id.clone(), object_id, activity)
        .await
}

#[tracing::instrument(level = "info", skip(state, activity), err)]
async fn handle_follow(
    actor: &Actor,
    activity: Value,
    host: &str,
    state: Arc<State>,
) -> Result<()> {
    let actor_id = actor.id.as_ref().ok_or(Error::StatusAndMessage {
        status: StatusCode::BAD_REQUEST,
        message: "actor has no id",
    })?;
    let inbox = actor.inbox.as_ref().ok_or(Error::StatusAndMessage {
        status: StatusCode::BAD_REQUEST,
        message: "actor has no inbox",
    })?;
    if state.db.add_inbox_if_unknown(inbox.to_owned())? {
        // New inbox so follow the remote actor
        state.client.follow_actor(actor_id).await?;
    }

    let our_actor = format!("https://{}/actor", state.cfg.base_url());
    let object_id = id_from_json(&activity);
    let message_id = Uuid::new_v4();

    let message = ActivityBuilder::new(String::from("Accept"), String::from("accepting follow"))
        .to(vec![actor_id.clone()])
        .object(
            // FIXME: object does not have a property "actor":
            // https://www.w3.org/TR/activitystreams-vocabulary/#types
            // .actor
            // object does not have a property "object":
            // https://www.w3.org/TR/activitystreams-vocabulary/#types
            // .object
            ObjectBuilder::new().id(object_id
                .parse::<http::Uri>()
                .map_err(|_e| Error::InvalidUri { uri: object_id })?),
        )
        .actor(
            ActorBuilder::new(String::from("Actor")).url(
                our_actor
                    .parse::<http::Uri>()
                    .map_err(|_e| Error::InvalidUri { uri: our_actor })?,
            ),
        )
        .id(format!("https://{host}/activities/{message_id}")
            .parse::<http::Uri>()
            .map_err(|_e| Error::StatusAndMessage {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: "failed to create parseable message id",
            })?)
        .build();

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

    let actor_id = actor.id.as_ref().ok_or(Error::StatusAndMessage {
        status: StatusCode::BAD_REQUEST,
        message: "actor has no id",
    })?;

    match ty.as_ref() {
        "Follow" => {
            state.db.remove_inbox(actor_id)?;
            state.client.unfollow_actor(actor_id).await
        }

        "Announce" => handle_forward(actor, activity, state).await,

        _ => Ok(()),
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    use crate::signature::tests::test_actor;
    use crate::state::Db;

    use simple_test_case::test_case;
    use std::{env::temp_dir, fs::remove_dir_all};

    #[test_case("Accept"; "accept")]
    #[test_case("Announce"; "announce")]
    #[test_case("Create"; "create")]
    #[test_case("Delete"; "delete")]
    #[test_case("Undo"; "undo")]
    #[test_case("Update"; "update")]
    #[tokio::test]
    async fn non_follow_for_unknown_inbox_is_an_error(ty: &str) {
        let mut dir = temp_dir();
        dir.push(Uuid::new_v4().to_string());

        let db = Db::new(dir.clone()).expect("unable to create database");
        let state = State::new_with_test_key(db);
        let res = validate_request(&test_actor("https://example.com/actor"), ty, &state).await;

        assert_eq!(
            res,
            Err(Error::StatusAndMessage {
                status: StatusCode::UNAUTHORIZED,
                message: "access denied"
            })
        );

        state.clear();
        remove_dir_all(dir).expect("to be able to clear up our temp directory");
    }

    #[tokio::test]
    async fn follow_for_unknown_inbox_is_ok() {
        let mut dir = temp_dir();
        dir.push(Uuid::new_v4().to_string());

        let db = Db::new(dir.clone()).expect("unable to create database");
        let state = State::new_with_test_key(db);
        let res =
            validate_request(&test_actor("https://example.com/actor"), "Follow", &state).await;

        assert_eq!(res, Ok(()));
        state.clear();
        remove_dir_all(dir).expect("to be able to clear up our temp directory");
    }

    #[test_case("Accept"; "accept")]
    #[test_case("Announce"; "announce")]
    #[test_case("Create"; "create")]
    #[test_case("Delete"; "delete")]
    #[test_case("Undo"; "undo")]
    #[test_case("Update"; "update")]
    #[tokio::test]
    async fn non_follow_for_known_inbox_is_ok(ty: &str) {
        let mut dir = temp_dir();
        dir.push(Uuid::new_v4().to_string());

        let db = Db::new(dir.clone()).expect("unable to create database");
        let state = State::new_with_test_key(db);
        state
            .db
            .add_inbox_if_unknown("https://example.com/actor".to_owned())
            .unwrap();

        let res = validate_request(&test_actor("https://example.com/actor"), ty, &state).await;

        assert_eq!(res, Ok(()));
        state.clear();
        remove_dir_all(dir).expect("to be able to clear up our temp directory");
    }
}
