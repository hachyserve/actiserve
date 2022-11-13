use crate::{base_url, nodeinfo::NODE_INFO_SCHEMA, Error, Jrd, Result};
use axum::{extract::Query, http::header, response::IntoResponse};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

pub async fn nodeinfo() -> Jrd<Value> {
    let base = base_url();
    let body = json!({
        "links": [
            {
                "rel": NODE_INFO_SCHEMA,
                "href": format!("{base}/nodeinfo/2.0"),
            }
        ]
    });

    Jrd(body)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    aliases: Vec<String>,
    links: Vec<Link>,
    subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    href: String,
    rel: String,
    #[serde(rename = "type")]
    ty: String,
}

// TODO: support rel?
#[derive(Debug, Deserialize)]
pub struct Params {
    resource: String,
}

// https://tools.ietf.org/html/rfc7033
pub async fn webfinger(Query(Params { resource }): Query<Params>) -> Result<Jrd<Resource>> {
    let (username, domain) = parse_webfinger_resource(&resource)?;
    let href = get_href(username, domain);
    let aliases = get_aliases(username, domain);

    Ok(Jrd(Resource {
        aliases,
        subject: resource.clone(),
        links: vec![
            Link {
                href: href.clone(),
                rel: "https://webfinger.net/rel/profile-page".to_owned(),
                ty: "application/activity+json".to_owned(),
            },
            Link {
                href,
                rel: "self".to_owned(),
                ty: "application/activity+json".to_owned(),
            },
        ],
    }))
}

// FIXME: actually look up the details
fn get_href(_username: &str, domain: &str) -> String {
    format!("https://{domain}/actor")
}

// FIXME: actually look up the details
fn get_aliases(_username: &str, _domain: &str) -> Vec<String> {
    vec![]
}

// parse a resource param of the form: /.well-known/webfinger?resource=acct:bob@my-example.com
fn parse_webfinger_resource(resource: &str) -> Result<(&str, &str)> {
    let uri = match resource.strip_prefix("acct:") {
        Some(s) => s,

        None => {
            return Err(Error::MalformedWebfingerResource {
                resource: resource.to_owned(),
            })
        }
    };

    let parts: Vec<&str> = uri.split('@').collect();
    if parts.len() != 2 {
        return Err(Error::MalformedWebfingerUri {
            uri: uri.to_owned(),
        });
    };

    let username = parts[0];
    let domain = parts[1];

    Ok((username, domain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use simple_test_case::test_case;

    #[test_case("acct:alice@example.com", Ok(("alice", "example.com")); "valid")]
    #[test_case("alice@example.com", Err(Error::MalformedWebfingerResource { resource: "alice@example.com".into() }); "missing prefix")]
    #[test_case("acct:alice@example@com", Err(Error::MalformedWebfingerUri { uri: "alice@example@com".into() }); "multiple at")]
    #[test_case("acct:alice.example.com", Err(Error::MalformedWebfingerUri { uri: "alice.example.com".into() }); "no at")]
    #[test]
    fn parse_webfinger_resource_works(resource: &str, expected: Result<(&str, &str)>) {
        let res = parse_webfinger_resource(resource);

        assert_eq!(res, expected)
    }
}
