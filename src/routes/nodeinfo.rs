//! Support for providing nodeinfo on /nodeinfo/2.0
//!
//! The schema for the reponse format can be found here:
//!   http://nodeinfo.diaspora.software/ns/schema/2.0#
use crate::State;
use axum::{extract::Json, http::header, response::IntoResponse, Extension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

pub const NODE_INFO_SCHEMA: &str = "http://nodeinfo.diaspora.software/ns/schema/2.0";

pub async fn get(Extension(state): Extension<Arc<State>>) -> impl IntoResponse {
    let headers = [(
        header::CONTENT_TYPE,
        format!("application/json; profile={NODE_INFO_SCHEMA}#,"),
    )];

    (headers, Json(NodeInfo::new(&state)))
}

/// NodeInfo schema version 2.0
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInfo {
    version: &'static str,
    software: Software,
    protocols: Vec<Protocol>,
    services: Services,
    open_registrations: bool,
    usage: UsageStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    meta_data: Option<Value>,
}

impl NodeInfo {
    pub fn new(state: &State) -> Self {
        Self {
            version: "2.0",
            software: Software::from_env(),
            protocols: vec![Protocol::ActivityPub],
            services: Services::default(),
            open_registrations: false, // TODO: double check what we should return here as a relay
            usage: UsageStats::new(state),
            meta_data: None,
        }
    }
}

/// Metadata about server software in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Software {
    name: &'static str,
    version: &'static str,
}

impl Software {
    fn from_env() -> Self {
        Self {
            name: "actiserve",
            version: option_env!("CARGO_PKG_VERSION").unwrap_or("unknown"),
        }
    }
}

/// Protocols that can be supported on this server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    ActivityPub,
    BuddyCloud,
    Dfrn,
    Diaspora,
    LiberTree,
    Ostatus,
    Pumpio,
    Tent,
    Xmpp,
    Zot,
}

// TODO: Does this need to be implemented using the enums below? We're allowed to return an
//       empty array for both of these. (If we are, then what services do we want to support?)

/// The third party sites this server can connect to via their application API.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Services {
    inbound: Vec<InboundService>,
    outbound: Vec<OutboundService>,
}

/// The third party sites this server can retrieve messages from for combined display with regular traffic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InboundService {
    #[serde(rename = "atom1.0")]
    Atom,
    GnuSocial,
    Imap,
    Pnut,
    Pop3,
    Pumpio,
    #[serde(rename = "rss2.0")]
    Rss,
    Twitter, // rip
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutboundService {
    #[serde(rename = "atom1.0")]
    Atom,
    Blogger,
    BuddyCloud,
    Diaspora,
    DreamWidth,
    Dripal,
    Facebook,
    Friendica,
    GnuSocial,
    Google,
    InsaneJournal,
    LiberTree,
    LinkedIn,
    LiveJournal,
    MediaGoblin,
    MySpace,
    Pinterest,
    Pnut,
    Posterous,
    Pumpio,
    RedMatrix,
    #[serde(rename = "rss2.0")]
    Rss,
    Smtp,
    Tent,
    Tumbler,
    Twitter, // rip
    Wordpress,
    Xmpp,
}

// NOTE: the only required field for the spec is users but we might want to provide
//       more later once more of the server is implemented.

/// Usage statistics for this server
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStats {
    users: UserStats,
    // local_posts: u32,
    // local_comments: u32,
}

impl UsageStats {
    // TODO: lookup user stats from persitent state / cache
    fn new(_state: &State) -> Self {
        Self {
            users: UserStats { total: 0 },
        }
    }
}

/// Statistics about the users of this server
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStats {
    total: u32,
    // active_half_year: u32,
    // active_month: u32,
}
