use std::net::SocketAddr;

use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, Request},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use miette::IntoDiagnostic;
use tracing::{debug, info};

const PROXY_FROM_DOMAIN: &str = "slow.delivrs.test";
const PROXY_ORIGIN_DOMAIN: &str = "localhost:8080";

#[tokio::main]
async fn main() -> miette::Result<()> {
    tracing_subscriber::fmt::init();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let app = Router::new().route("/", get(index)).fallback(proxy_request);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .into_diagnostic()?;

    debug!("listening on {}", addr);
    axum::serve(listener, app).await.into_diagnostic()?;

    Ok(())
}

async fn index() -> impl IntoResponse {
    println!("->> HANDLER - index");
    "Hello, world"
}

async fn proxy_request(request: Request<Body>) -> miette::Result<impl IntoResponse, String> {
    info!("HANDLER - proxy_request: {:?}", request);

    let uri = request.uri();
    let host = request
        .headers()
        .get("host")
        .ok_or("No host specified")?
        .to_str()
        .map_err(|e| format!("Could not parse host header: {}", e))?;

    if host != PROXY_FROM_DOMAIN {
        return Err(format!(
            "Requests are only proxied from specified domain. Found: {} - Expected: {}",
            host, PROXY_FROM_DOMAIN
        ));
    }

    let path = uri.path_and_query().map(|pq| pq.path()).unwrap_or("/");
    let client = reqwest::Client::new();
    let reqw_response = client
        .get(format!("http://{}{}", PROXY_ORIGIN_DOMAIN, path))
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    let mut response_builder = Response::builder().status(reqw_response.status().as_u16());
    response_builder.headers_mut().map(|headers| {
        headers.extend(reqw_response.headers().into_iter().map(|(name, value)| {
            let name = HeaderName::from_bytes(name.as_ref()).unwrap();
            let value = HeaderValue::from_bytes(value.as_ref()).unwrap();
            (name, value)
        }))
    });

    let response = response_builder
        .body(Body::from(
            reqw_response
                .bytes()
                .await
                .into_diagnostic()
                .map_err(|_| "failed to get bytes from header")?,
        ))
        .map_err(|_| "failed to set body")?;

    Ok(response)
}
