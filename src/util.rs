//! Utility functions
use crate::{Error, Result};
use axum::http::{HeaderValue, StatusCode, Uri};
use serde_json::Value;

#[macro_export]
macro_rules! map {
    {} => { ::std::collections::HashMap::new() };

    {
        map_keys: $mapper:expr;
        $($key:expr => $value:expr),+,
    } => {
        {
            let mut _map: ::std::collections::HashMap<_, _> = ::std::collections::HashMap::new();
            $(_map.insert($mapper($key), $value);)+
            _map
        }
    };

    { $($key:expr => $value:expr),+, } => {
        {
            let mut _map: ::std::collections::HashMap<_, _> = ::std::collections::HashMap::new();
            $(_map.insert($key, $value);)+
            _map
        }
    };
}

pub fn host_from_uri(uri: &str) -> Result<String> {
    let parsed = uri.parse::<Uri>().map_err(|_| Error::InvalidUri {
        uri: uri.to_owned(),
    })?;

    let host = parsed.host().ok_or_else(|| Error::InvalidUri {
        uri: uri.to_owned(),
    })?;

    Ok(host.to_owned())
}

pub fn id_from_json(val: &Value) -> String {
    let obj = &val["object"];

    let id = match obj.get("id") {
        Some(id) => id.as_str(),
        None => obj.as_str(),
    };

    id.unwrap().to_owned()
}

// We should never be trying to construct an invalid header value in sign_request
// below so if this pops we've definitely messed up somewhere
pub fn header_val(s: &str) -> Result<HeaderValue> {
    HeaderValue::from_str(s).map_err(|_| Error::StatusAndMessage {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        message: "internal server error",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use simple_test_case::test_case;

    #[test_case("https://example.com/foo/bar"; "https")]
    #[test_case("http://example.com/foo/bar"; "http")]
    #[test_case("https://example.com"; "no path")]
    #[test]
    fn host_from_uri_parses_a_valid_uri(uri: &str) {
        let res = host_from_uri(uri);

        assert_eq!(res.as_deref(), Ok("example.com"));
    }

    #[test]
    fn host_from_uri_rejects_an_invalid_uri() {
        let uri = "example.com/foo/bar";
        let res = host_from_uri(uri);

        assert_eq!(
            res,
            Err(Error::InvalidUri {
                uri: uri.to_owned()
            })
        );
    }
}
