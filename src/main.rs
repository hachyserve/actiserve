use axum::Server;
use std::{
    collections::HashMap,
    net::SocketAddr,
    panic,
    sync::{Arc, Mutex},
};
use tracing::{error, info, subscriber};
use tracing_subscriber::EnvFilter;

mod error;
mod nodeinfo;
mod routes;
mod statuses;

pub use error::Error;
use routes::build_routes;
use statuses::Status;

const PORT: u16 = 4242;

/// Lookup our base url from the environment or default to localhost:4242
pub fn base_url() -> &'static str {
    option_env!("BASE_URL").unwrap_or("127.0.0.1:4242")
}

// TODO: persistent store for the statuses
pub type State = Arc<Mutex<HashMap<String, Status>>>;

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

    let state: State = Arc::new(Mutex::new(HashMap::new()));
    let app = build_routes(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("server to start");
}
