#[macro_use]
extern crate actix_web;
#[macro_use]
extern crate lazy_static;

use actix_web::{middleware, App, HttpServer};
use std::{env, io};

mod constants;
mod statuses;

#[actix_rt::main]
async fn main() -> io::Result<()> {
    env::set_var("RUST_LOG", "actix_web=debug,actix_server=debug");
    env_logger::init();

    println!("starting service on :4242");

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            .service(statuses::create)
            .service(statuses::get)
            .service(statuses::delete)
    })
    .bind("0.0.0.0:4242")?
    .run()
    .await
}
