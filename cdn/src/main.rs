use std::net::SocketAddr;

use axum::{response::IntoResponse, routing::get, Router};
use miette::IntoDiagnostic;
use tracing::debug;

#[tokio::main]
async fn main() -> miette::Result<()> {
    tracing_subscriber::fmt::init();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let app = Router::new().route("/", get(index));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    debug!("listening on {}", addr);
    axum::serve(listener, app).await.into_diagnostic()?;

    Ok(())
}

async fn index() -> impl IntoResponse {
    "Hello, world"
}
