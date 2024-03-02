use std::{net::SocketAddr, str::FromStr};

use axum::{
    body::Body,
    extract::Host,
    http::{HeaderMap, HeaderName, HeaderValue, Request},
    response::{IntoResponse, Response},
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
    let app = Router::new().fallback(proxy_request);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .into_diagnostic()?;

    debug!("listening on {}", addr);
    axum::serve(listener, app).await.into_diagnostic()?;

    Ok(())
}

async fn proxy_request(
    Host(host): Host,
    headers: HeaderMap,
    request: Request<Body>,
) -> miette::Result<impl IntoResponse, String> {
    info!("HANDLER - proxy_request: {:?}", request);

    let mut split = host.split(':');
    let hostname = split.next().unwrap_or("unknown");

    if hostname != PROXY_FROM_DOMAIN {
        return Err(format!(
            "Requests are only proxied from specified domain. Found: {} - Expected: {}",
            hostname, PROXY_FROM_DOMAIN
        ));
    }

    let path = request
        .uri()
        .path_and_query()
        .map(|pq| pq.path())
        .unwrap_or("/");

    let client = reqwest::Client::new();
    let reqw_response = client
        .request(
            reqwest::Method::from_str(&request.method().to_string()).unwrap(),
            format!("http://{}{}", PROXY_ORIGIN_DOMAIN, path),
        )
        .headers(map_to_reqwest_headers(headers))
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    let response = into_axum_response(reqw_response).await?;

    Ok(response)
}

fn map_to_reqwest_headers(headers: HeaderMap) -> reqwest::header::HeaderMap {
    let mut reqwest_headers = reqwest::header::HeaderMap::with_capacity(headers.len());
    reqwest_headers.extend(headers.into_iter().map(|(name, value)| {
        let name = reqwest::header::HeaderName::from_bytes(name.unwrap().as_ref()).unwrap();
        let value = reqwest::header::HeaderValue::from_bytes(value.as_ref()).unwrap();

        (name, value)
    }));

    reqwest_headers
}

async fn into_axum_response(
    response: reqwest::Response,
) -> miette::Result<impl IntoResponse, String> {
    let mut response_builder = Response::builder().status(response.status().as_u16());
    response_builder.headers_mut().map(|headers| {
        headers.extend(response.headers().into_iter().map(|(name, value)| {
            let name = HeaderName::from_bytes(name.as_ref()).unwrap();
            let value = HeaderValue::from_bytes(value.as_ref()).unwrap();
            (name, value)
        }))
    });

    let response = response_builder
        .body(Body::from(
            response
                .bytes()
                .await
                .into_diagnostic()
                .map_err(|_| "failed to get bytes from header")?,
        ))
        .map_err(|_| "failed to set body")?;

    Ok(response)
}
