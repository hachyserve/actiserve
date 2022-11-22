//! Simple smoke test checks that endpoints are healthy
use reqwest::{Client, StatusCode};
use simple_test_case::test_case;

#[test_case(".well-known/webfinger?resource=acct:relay@127.0.0.1:4242"; "webfinger")]
#[test_case(".well-known/nodeinfo"; "well known node info")]
#[test_case(".well-known/host-meta"; "host meta")]
#[test_case("nodeinfo/2.0"; "node info")]
#[test_case("actor"; "actor")]
#[cfg_attr(not(feature = "need_local_server"), ignore)]
#[tokio::test]
async fn happy_path_get(uri: &str) -> anyhow::Result<()> {
    let base = option_env!("BASE_URL").unwrap_or("http://127.0.0.1:4242");

    let client = Client::new();
    let res = client.get(format!("{base}/{uri}")).send().await?;

    assert_eq!(res.status(), StatusCode::OK);

    Ok(())
}
