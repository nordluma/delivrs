use std::{net::SocketAddr, str::FromStr};

use axum::{
    body::{Body, Bytes},
    extract::Host,
    http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Uri},
    response::{IntoResponse, Response},
    Router,
};
use futures::StreamExt;
use miette::IntoDiagnostic;
use reqwest::{
    header::{
        HeaderMap as ReqHeaderMap, HeaderName as ReqHeaderName, HeaderValue as ReqHeaderValue,
    },
    Method as ReqMethod,
};
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
    method: Method,
    headers: HeaderMap,
    request: Request<Body>,
) -> miette::Result<impl IntoResponse, String> {
    info!("HANDLER - proxy_request: {:?}", request);

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
        .path_and_query(
            request
                .uri()
                .path_and_query()
                .map(|pq| pq.path())
                .unwrap_or("/"),
        )
        .build()
        .map_err(|e| format!("Failed to build url: {}", e))?;

    let response = try_get_cached_response(&method, &headers, &url, request.into_body()).await?;

    //let response = into_axum_response(reqw_response).await?;

    Ok(response)
}

async fn try_get_cached_response(
    method: &Method,
    headers: &HeaderMap,
    url: &Uri,
    body: Body,
) -> miette::Result<impl IntoResponse, String> {
    let client = reqwest::Client::new();

    let response = client
        .request(
            ReqMethod::from_str(method.as_str()).unwrap(),
            url.to_string(),
        )
        .headers(map_to_reqwest_headers(headers.clone()))
        .body(body_to_bytes(body).await?)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    Ok(into_axum_response(response).await?)
}

fn map_to_reqwest_headers(headers: HeaderMap) -> ReqHeaderMap {
    let mut reqwest_headers = ReqHeaderMap::with_capacity(headers.len());
    reqwest_headers.extend(headers.into_iter().map(|(name, value)| {
        // TODO: change unwrap to something better
        let name = ReqHeaderName::from_bytes(name.unwrap().as_ref()).unwrap();
        let value = ReqHeaderValue::from_bytes(value.as_ref()).unwrap();

        (name, value)
    }));

    reqwest_headers
}

async fn body_to_bytes(body: Body) -> miette::Result<Vec<u8>, String> {
    let mut body_bytes = Vec::new();
    let mut body = body.into_data_stream();
    while let Some(bytes) = body.next().await {
        let bytes = bytes.map_err(|e| format!("Failed the get bytes for body: {}", e))?;
        body_bytes.extend(bytes);
    }

    Ok(body_bytes)
}

async fn response_body_to_bytes(response: reqwest::Response) -> miette::Result<Bytes, String> {
    response
        .bytes()
        .await
        .into_diagnostic()
        .map_err(|e| format!("failed to get bytes from response: {}", e))
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
        .body(Body::from(response_body_to_bytes(response).await?))
        .map_err(|e| format!("failed to set response body: {}", e))?;

    Ok(response)
}
