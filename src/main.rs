use axum::Server;
use clap::Parser;
use std::{net::SocketAddr, panic, path::PathBuf, sync::Arc};
use tracing::{error, info, subscriber};
use tracing_subscriber::EnvFilter;

use actiserve::{
    config::Config,
    routes::build_routes,
    state::{Db, State},
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the YAML config file to use
    #[arg(long, default_value = "config.yaml")]
    config_path: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let cfg = Config::load_or_write_default(args.config_path);

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

    run_server(cfg).await
}

async fn run_server(cfg: Config) {
    info!(path = %cfg.private_key_path.display(), "loading private key");
    let priv_key_pem =
        std::fs::read_to_string(&cfg.private_key_path).expect("unable to read private key");

    info!(
        data_dir = %cfg.data_dir.display(),
        "initialising DB"
    );
    let db = Db::new(cfg.data_dir.clone()).expect("unable to create database");

    let addr: SocketAddr = cfg
        .base_url()
        .parse()
        .expect("unable to parse address and port");
    let port = cfg.port;

    let state: Arc<State> = Arc::new(State::new(cfg, db, &priv_key_pem));
    let app = build_routes(state);

    info!(%port, "starting service");
    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("server to start");
}
