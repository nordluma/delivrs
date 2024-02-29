use std::net::SocketAddr;

use axum::{response::IntoResponse, routing::get, Router};
use tracing::debug;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let app = Router::new().route("/", get(index));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    debug!("listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> impl IntoResponse {
    "Hello, world"
}
