use axum::Server;
use std::{net::SocketAddr, panic, sync::Arc};
use tracing::{error, info, subscriber};
use tracing_subscriber::EnvFilter;

mod client;
mod error;
mod routes;
mod signature;
mod state;
mod util;

pub use error::{Error, Result};
use routes::build_routes;
use state::State;

const PORT: u16 = 4242;

/// Lookup our base url from the environment or default to localhost:4242
pub fn base_url() -> &'static str {
    option_env!("BASE_URL").unwrap_or("127.0.0.1:4242")
}

#[tokio::main]
async fn main() {
    subscriber::set_global_default(
        tracing_subscriber::fmt()
            .json()
            .flatten_event(true)
            .with_env_filter(EnvFilter::from_default_env())
            .finish(),
    )
    .expect("this to be the only global subscriber");

    panic::set_hook(Box::new(|panic| {
        if let Some(location) = panic.location() {
            error!(
                message=%panic,
                panic.file=location.file(),
                panic.line=location.line(),
                panic.column=location.column()
            );
        } else {
            error!(message=%panic)
        }
    }));

    run_server().await
}

async fn run_server() {
    info!(port = PORT, "starting service");

    let priv_key_pem = match std::fs::read_to_string("priv-key.pem") {
        Ok(s) => s,
        Err(e) => panic!("unable to read private key: {e}"),
    };

    let state: Arc<State> = Arc::new(State::new(Default::default(), &priv_key_pem));
    let app = build_routes(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("server to start");
}
