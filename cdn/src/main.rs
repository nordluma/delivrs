use std::{collections::HashMap, net::SocketAddr, str::FromStr, sync::Mutex, time::SystemTime};

use axum::{
    body::{Body, Bytes},
    debug_handler,
    extract::Host,
    http::{self, Method, Request, Uri},
    response::IntoResponse,
    Router,
};
use http_cache_semantics::{BeforeRequest, CachePolicy, RequestLike};
use miette::{miette, IntoDiagnostic};
use reqwest::Method as ReqMethod;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::utils::{
    body_to_bytes, bytes_to_body, into_axum_response, map_to_reqwest_headers, response_with_headers,
};

mod utils;

const PROXY_FROM_DOMAIN: &str = "slow.delivrs.test";
const PROXY_ORIGIN_DOMAIN: &str = "localhost:8080";
const CACHE_DIR: &str = "./tmp/cache";

type CacheKey = (Method, Uri);
type Cache = Mutex<HashMap<CacheKey, CachedResponse>>;

lazy_static::lazy_static! {
    static ref CACHE: Cache = Mutex::new(HashMap::new());
}

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
#[tracing::instrument(skip_all)]
async fn proxy_request(
    Host(host): Host,
    request: Request<Body>,
) -> miette::Result<impl IntoResponse, String> {
    let now = SystemTime::now();
    info!("request received at: {:?}", now);
    let hostname = host.split(':').next().unwrap_or("unknown");
    if hostname != PROXY_FROM_DOMAIN {
        return Err(format!(
            "Requests are only proxied from specified domain. Found: {} - Expected: {}",
            hostname, PROXY_FROM_DOMAIN
        ));
    }

    try_get_cached_response(request, now)
        .await
        .and_then(bytes_to_body)
        .map_err(|e| e.to_string())
}

trait IntoCacheable {
    type Cacheable;

    fn into_cacheable(self) -> Self::Cacheable;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct InnerCacheRequest {
    #[serde(with = "http_serde::method")]
    method: http::Method,

    #[serde(with = "http_serde::uri")]
    uri: http::Uri,

    #[serde(with = "http_serde::version")]
    version: http::Version,

    #[serde(with = "http_serde::header_map")]
    headers: http::HeaderMap,

    body: Vec<u8>,
}

impl IntoCacheable for http::Request<Bytes> {
    type Cacheable = InnerCacheRequest;

    fn into_cacheable(self) -> Self::Cacheable {
        let (parts, body) = self.into_parts();

        InnerCacheRequest {
            method: parts.method,
            uri: parts.uri,
            version: parts.version,
            headers: parts.headers,
            body: body.to_vec(),
        }
    }
}

impl TryFrom<InnerCacheRequest> for http::Request<Bytes> {
    type Error = miette::Error;

    fn try_from(request: InnerCacheRequest) -> Result<Self, Self::Error> {
        let mut request_builer = http::Request::builder()
            .method(request.method)
            .uri(request.uri)
            .version(request.version);

        for (key, value) in request.headers {
            if let Some(key) = key {
                request_builer = request_builer.header(key, value);
            }
        }

        request_builer
            .body(Bytes::from(request.body))
            .map_err(|e| miette!("Failed to set request body: {}", e))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct InnerCacheResponse {
    #[serde(with = "http_serde::status_code")]
    status: http::StatusCode,

    #[serde(with = "http_serde::version")]
    version: http::Version,

    #[serde(with = "http_serde::header_map")]
    headers: http::HeaderMap,

    body: Vec<u8>,
}

impl IntoCacheable for http::Response<Bytes> {
    type Cacheable = InnerCacheResponse;

    fn into_cacheable(self) -> Self::Cacheable {
        let (parts, body) = self.into_parts();

        InnerCacheResponse {
            status: parts.status,
            version: parts.version,
            headers: parts.headers,
            body: body.to_vec(),
        }
    }
}

impl TryFrom<InnerCacheResponse> for http::Response<Bytes> {
    type Error = miette::Error;
    fn try_from(response: InnerCacheResponse) -> Result<Self, Self::Error> {
        let mut response_builder = http::Response::builder()
            .status(response.status)
            .version(response.version);
        response_builder = response_with_headers(response_builder, &response.headers);

        response_builder
            .body(Bytes::from(response.body))
            .map_err(|e| miette!("Failed to set body for response: {}", e))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CachedResponse {
    request: InnerCacheRequest,
    response: InnerCacheResponse,
    cached_at: SystemTime,
}

impl CachedResponse {
    fn new(request: http::Request<Bytes>, response: http::Response<Bytes>) -> Self {
        Self {
            request: request.into_cacheable(),
            response: response.into_cacheable(),
            cached_at: SystemTime::now(),
        }
    }
}

#[tracing::instrument(skip_all)]
async fn try_get_cached_response(
    request: Request<Body>,
    response_time: SystemTime,
) -> miette::Result<http::Response<Bytes>> {
    info!("Request headers: {:?}", request.headers());
    let url = request.uri().clone();

    {
        //let cache = CACHE.lock().unwrap();
        //let cached_response = cache.get(&(request.method().clone(), url.clone()));
        let cache_key = format!("{}@{}", request.method(), url);
        let cache_res = cacache::read(CACHE_DIR, cache_key)
            .await
            .map_err(|e| miette!("failed to read from cache: {}", e))?;

        let cached_response: Option<CachedResponse> = postcard::from_bytes(&cache_res)
            .map_err(|e| miette!("Failed to deserialize: {}", e))?;

        if let Some(cached) = cached_response {
            let cached_request: http::Request<Bytes> = cached.request.try_into()?;
            let cached_response: http::Response<Bytes> = cached.response.try_into()?;

            let policy = CachePolicy::new_options(
                &cached_request,
                &cached_response,
                response_time,
                Default::default(),
            );

            match policy.before_request(&request, SystemTime::now()) {
                BeforeRequest::Fresh(_) => {
                    info!("Cache hit for: {}", url);
                    return Ok(cached_response.clone());
                }
                BeforeRequest::Stale {
                    request: new_request,
                    matches,
                } => {
                    info!(
                        matches,
                        cached_at = ?cached.cached_at,
                        cache_control = ?new_request.headers().get("Cache-Control"),
                        "Cache hit but response is stale: {}",
                        url
                    );
                }
            }
        }
    }

    info!("Cache miss for: {}", url);
    let origin_url = Uri::builder()
        .scheme("http")
        .authority(PROXY_ORIGIN_DOMAIN)
        .path_and_query(url.path_and_query().map(|pq| pq.path()).unwrap_or("/"))
        .build()
        .map_err(|e| miette!("Failed to build url: {}", e))?;

    let request = body_to_bytes(request).await?;
    let client = reqwest::Client::new();
    let origin_response = client
        .request(
            ReqMethod::from_str(request.method().as_str()).unwrap(),
            origin_url.to_string(),
        )
        .headers(map_to_reqwest_headers(request.headers().clone()))
        .header("Age", 0) // Insert missing age header
        .body(request.body().clone())
        .send()
        .await
        .map_err(|e| miette!("Request failed: {}", e))?;

    let response = {
        let response = into_axum_response(origin_response).await?;
        let cache_key = format!("{}@{}", request.method(), url);
        let mut buf = vec![];
        let ser = postcard::to_slice(&CachedResponse::new(request, response.clone()), &mut buf)
            .map_err(|e| miette!("failed to serialize: {}", e))?;
        cacache::write(CACHE_DIR, cache_key, ser)
            .await
            .map_err(|e| miette!("failed to write to cache: {}", e))?;

        response
    };

    Ok(response)
}
