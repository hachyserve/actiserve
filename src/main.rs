use axum::Server;
use clap::Parser;
use routes::build_routes;
use std::{net::SocketAddr, panic, path::PathBuf, sync::Arc};
use tracing::{error, info, subscriber};
use tracing_subscriber::EnvFilter;

use crate::state::{Db, State};

mod client;
mod error;
mod routes;
mod signature;
mod state;
mod util;

pub use error::{Error, Result};

// TODO: move this to Args
const PORT: u16 = 4242;

/// Lookup our base url from the environment or default to localhost:4242
pub fn base_url() -> &'static str {
    // TODO: move this to Args or build it from there.
    option_env!("BASE_URL").unwrap_or("127.0.0.1:4242")
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Directory to use for storing JSON DB state
    #[arg(long, default_value = ".")]
    data_dir: PathBuf,

    /// Path to a valid private key in PEM format
    #[arg(long)]
    private_key: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

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

    run_server(args).await
}

async fn run_server(args: Args) {
    info!(path = %args.private_key.display(), "loading private key");
    let priv_key_pem =
        std::fs::read_to_string(args.private_key).expect("unable to read private key");

    info!(
        data_dir = %args.data_dir.display(),
        "initialising DB"
    );
    let db = Db::new(args.data_dir).expect("unable to create database");

    let state: Arc<State> = Arc::new(State::new(db, &priv_key_pem));
    let app = build_routes(state);

    let addr: SocketAddr = base_url()
        .parse()
        .expect("unable to parse address and port");

    info!(port = PORT, "starting service");

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("server to start");
}
