use std::{net::SocketAddr, str::FromStr};

use axum::{
    body::Bytes,
    debug_handler,
    extract::Host,
    http::{self, HeaderMap, Method, Uri},
    response::IntoResponse,
    Router,
};
use miette::IntoDiagnostic;
use reqwest::Method as ReqMethod;
use tracing::{debug, info};
use utils::{into_axum_response, map_to_reqwest_headers};

mod utils;

const PROXY_FROM_DOMAIN: &str = "slow.delivrs.test";
const PROXY_ORIGIN_DOMAIN: &str = "localhost:8080";

#[tokio::main]
async fn main() -> miette::Result<()> {
    tracing_subscriber::fmt::init();

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let app = Router::new().fallback(proxy_request);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .into_diagnostic()?;

    debug!("listening on {}", addr);
    axum::serve(listener, app).await.into_diagnostic()?;

    Ok(())
}

#[debug_handler]
async fn proxy_request(
    Host(host): Host,
    method: Method,
    url: Uri,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> miette::Result<impl IntoResponse, String> {
    info!("HANDLER - proxy_request");

    let hostname = host.split(':').next().unwrap_or("unknown");
    if hostname != PROXY_FROM_DOMAIN {
        return Err(format!(
            "Requests are only proxied from specified domain. Found: {} - Expected: {}",
            hostname, PROXY_FROM_DOMAIN
        ));
    }

    let url = Uri::builder()
        .scheme("http")
        .authority(PROXY_ORIGIN_DOMAIN)
        .path_and_query(url.path_and_query().map(|pq| pq.path()).unwrap_or("/"))
        .build()
        .map_err(|e| format!("Failed to build url: {}", e))?;

    let response = try_get_cached_response(&method, &headers, &url, body_bytes).await?;

    Ok(response)
}

async fn try_get_cached_response(
    method: &Method,
    headers: &HeaderMap,
    url: &Uri,
    body: Bytes,
) -> miette::Result<impl IntoResponse, String> {
    let client = reqwest::Client::new();

    let response = client
        .request(
            ReqMethod::from_str(method.as_str()).unwrap(),
            url.to_string(),
        )
        .headers(map_to_reqwest_headers(headers.clone()))
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    Ok(into_axum_response(response).await?)
}
