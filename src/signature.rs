use crate::{base_url, client::Actor, Error, Result};
use axum::http::{HeaderMap, Uri};
use chrono::Utc;
use itertools::Itertools;
use reqwest::StatusCode;
use rsa::{
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    signature::{RandomizedSigner, Signature as _, Verifier},
    RsaPublicKey,
};
use sha2::{Digest, Sha256, Sha512};
use std::{collections::HashMap, convert::TryInto};
use tracing::debug;

// If something was wrong with the signature we don't want to leak any details about
// why we have rejected it.
const INVALID_SIG: Error = Error::StatusAndMessage {
    status: StatusCode::UNAUTHORIZED,
    message: "invalid HTTP signature",
};

pub fn sign_request_headers(
    uri: &str,
    data: Option<&str>,
    sig_key: &SigningKey<Sha256>,
) -> Result<HeaderMap> {
    let uri = uri.parse::<Uri>().map_err(|_| Error::InvalidUri {
        uri: uri.to_owned(),
    })?;

    let method = if data.is_some() { "post" } else { "get" };
    let path = uri.path();
    let host = uri.host().ok_or(Error::InvalidUri {
        uri: uri.to_string(),
    })?;

    let mut headers: HashMap<String, String> = HashMap::new();
    headers.insert("(request-target)".into(), format!("{method} {path}"));
    headers.insert("Date".into(), Utc::now().to_string());
    headers.insert("Host".into(), host.into());

    if let Some(s) = data {
        headers.insert("Content-Length".into(), s.len().to_string());

        let h = hmac_sha256::Hash::hash(s.as_bytes());
        let digest = base64::encode(h);
        headers.insert("Digest".into(), format!("SHA-256={digest}"));
    }

    let signature = create_signature(&headers, sig_key);
    headers.insert("Signature".into(), signature);

    // Now that we've generated the signature we can remove what we no longer need
    headers.remove("(request-target)");
    headers.remove("Host");

    Ok((&headers).try_into().expect("valid headers"))
}

pub fn validate_signature(
    actor: &Actor,
    method: &str,
    path: &str,
    headers: &HeaderMap,
) -> Result<()> {
    let Some(sig) = headers.get("signature") else {
    	return Err(Error::MissingSignature);
    };
    let pub_key = actor.key()?;
    let mut sig = split_signature(sig.to_str().map_err(|_| INVALID_SIG)?)?;
    let target = format!("{method} {path}");
    sig.insert("(request-target)", &target);

    let string_sig = sig.get("signature").ok_or(INVALID_SIG)?;
    let sig_data = base64::decode(string_sig).map_err(|_| INVALID_SIG)?;
    let signature = Signature::from(sig_data);

    let ordered_headers: Vec<(&str, &str)> = sig
        .get("headers")
        .ok_or(INVALID_SIG)?
        .split(' ')
        .map(|k| sig.get(k).ok_or(INVALID_SIG).map(|v| (k, *v)))
        .collect::<Result<_>>()?;

    let signing_string = build_signing_string(&ordered_headers);

    let (_, hash_algorithm) = sig
        .get("algorithm")
        .ok_or(INVALID_SIG)?
        .split_once('-')
        .ok_or(INVALID_SIG)?;

    match hash_algorithm {
        // "sha1" => (),
        "sha256" => verify::<Sha256>(pub_key, signing_string.as_bytes(), &signature),
        "sha512" => verify::<Sha512>(pub_key, signing_string.as_bytes(), &signature),
        _ => Err(INVALID_SIG),
    }
}

fn verify<D: Digest>(pub_key: RsaPublicKey, data: &[u8], signature: &Signature) -> Result<()> {
    let verify_key: VerifyingKey<D> = pub_key.into();

    verify_key.verify(data, signature).map_err(|e| {
        debug!(%e, "invalid signature");
        INVALID_SIG
    })
}

fn create_signature(headers: &HashMap<String, String>, sig_key: &SigningKey<Sha256>) -> String {
    // Convert to a vec of pairs to ensure the iteration order is consistent in
    // both the signature and the list of used headers
    let pairs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let signed_bytes = sig_key
        .sign_with_rng(
            &mut rand::thread_rng(),
            build_signing_string(&pairs).as_bytes(),
        )
        .as_bytes()
        .to_vec();

    let signature = base64::encode(signed_bytes);

    build_sig_header(signature, pairs.into_iter().map(|(k, _)| k))
}

fn build_signing_string(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}: \"{v}\"",))
        .join("\n")
}

fn split_signature(s: &str) -> Result<HashMap<&str, &str>> {
    s.split(',')
        .map(|pair| {
            let (k, v) = pair.split_once('=').ok_or(INVALID_SIG)?;

            v.strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .map(|v| (k, v))
                .ok_or(INVALID_SIG)
        })
        .collect()
}

fn build_sig_header<'a>(signature: String, mut headers: impl Iterator<Item = &'a str>) -> String {
    let headers = headers.join(" ");

    vec![
        format!("keyId=\"https://{}/actor#main-key\"", base_url()),
        "algorithm=\"rsa-sha256\"".to_owned(),
        format!("headers=\"{headers}\""),
        format!("signature=\"{signature}\""),
    ]
    .join(",")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::map;
    use rsa::{pkcs1::DecodeRsaPrivateKey, RsaPrivateKey};

    // A valid but low bit size private key for use in running unit tests
    // without needing to generate one on demand.
    //
    // This should (obviously) never be moved out of this module with #[cfg(test)]
    pub const TEST_PRIV_KEY: &str = "\
-----BEGIN RSA PRIVATE KEY-----
MIICXAIBAAKBgQC+PFlNktFyu41p3QjbqprDXjh7RmjYNH7k0Mx4oGLzIXPGAFQu
iE24LST2pNu9SiOWJ/ul6NhPBlP5kRHxmcvxtO4lenqi3Isp23iYlae9SsVEdsf+
RkejKyRw1xH2LAs0opISN9yh4bMbtMn9evI5TaK5YE/GM2sdsuUJKam7RQIDAQAB
AoGAa1QDElgmITQdqb+SEtUjMdyDw1FLL8gWW6RN6DSc/w09k1V2KTavmpylwR3r
99TPVRVDziwbdiJc2G33kLazr7YWRvalazyU+U6Zz+OqzfLkVDx1BTl641d8eL2b
u9unqrPljnRivnhqCoI+z0y6cwpCa33zgb3SE+LfVgUcNpUCQQDu85UrzRUP2KsM
qNyLtbEOtbPsa4SSyPbc41sk+emha9Pv7dTbH4EJV1C71JFaufjrz1X8Zo7Kvj3K
t9gWBn03AkEAy876s+mBkpC1fk2U08N37uqJTMRjDrntK5bN4jIgf+FkSYog3XmK
iGMx2SZDutieET0iUdqxX2mrV+TnNnKpYwJAaxtEAh4rEq9L/KC0Out2MeHAhHit
NB5giSJf+HMNBg4PMbypbI7yh/1bctYVUVWK/igxorFV0Ar2J6fAdB70gQJAHhJu
P3mm2r9raDV+Tji7S49jruYTT6rzackYm9WVogjZyVgOPV+fpzwrsMTKnZk0yYph
s/42ycNHuvJVg10rzQJBALf3TTpmvPrZP0Oapq6LWWfJ1l2ykD7rgue3Uayxogtj
IoGq/6wrgUro6hOTiO9q82rUknQFF0nvc4ygu9+YrFs=
-----END RSA PRIVATE KEY-----";

    pub const TEST_PUB_KEY: &str = "\
-----BEGIN RSA PUBLIC KEY-----
MIGJAoGBAL48WU2S0XK7jWndCNuqmsNeOHtGaNg0fuTQzHigYvMhc8YAVC6ITbgt
JPak271KI5Yn+6Xo2E8GU/mREfGZy/G07iV6eqLciynbeJiVp71KxUR2x/5GR6Mr
JHDXEfYsCzSikhI33KHhsxu0yf168jlNorlgT8Yzax2y5QkpqbtFAgMBAAE=
-----END RSA PUBLIC KEY-----";

    pub fn sign_test_req(uri: &str, data: Option<&str>) -> HeaderMap {
        let s_key: SigningKey<Sha256> = RsaPrivateKey::from_pkcs1_pem(TEST_PRIV_KEY)
            .expect("test key to be valid")
            .into();

        sign_request_headers(uri, data, &s_key).expect("to sign")
    }

    #[test]
    fn signature_splitting_works() {
        let key = "https://example.com/actor#main-key";
        let headers = "foo bar baz";
        let alg = "rsa-sha256";
        let sig = "SIGNATURE";

        let signature = format!(
            "keyId=\"{key}\",algorithm=\"{alg}\",headers=\"{headers}\",signature=\"{sig}\""
        );
        let split = split_signature(&signature).expect("test signature to be valid");

        let expected = map! {
            "keyId" => key,
            "algorithm" => alg,
            "headers" => headers,
            "signature" => sig,
        };

        assert_eq!(split, expected);
    }

    #[test]
    fn we_can_verify_our_own_signatures() {
        let uri = "https://example.com/inbox";
        let data = Some(r#"{ "hello": "world" }"#);
        let headers = sign_test_req(uri, data);

        // Will provide the TEST_PUB_KEY public key for verification
        let actor = Actor::test_actor("https://example.com/actor");

        let res = validate_signature(&actor, "post", "/inbox", &headers);
        assert!(res.is_ok());
    }
}
