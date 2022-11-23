pub mod client;
pub mod config;
pub mod error;
pub mod routes;
pub mod signature;
pub mod state;
pub mod util;

pub use error::{Error, Result};

/// Lookup our base url from the environment or default to localhost:4242
pub fn base_url() -> &'static str {
    // TODO: move this to Args or build it from there.
    option_env!("BASE_URL").unwrap_or("127.0.0.1:4242")
}
