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
use miette::IntoDiagnostic;
use reqwest::Method as ReqMethod;
use tracing::{debug, info, warn};
use utils::{into_axum_response, map_to_reqwest_headers};

use crate::utils::{body_to_bytes, bytes_to_body, response_with_headers};

mod utils;

const PROXY_FROM_DOMAIN: &str = "slow.delivrs.test";
const PROXY_ORIGIN_DOMAIN: &str = "localhost:8080";

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
#[tracing::instrument(skip(request))]
async fn proxy_request(
    Host(host): Host,
    request: Request<Body>,
) -> miette::Result<impl IntoResponse, String> {
    info!("HANDLER - proxy_request");
    dbg!(request.headers());
    let hostname = host.split(':').next().unwrap_or("unknown");
    if hostname != PROXY_FROM_DOMAIN {
        return Err(format!(
            "Requests are only proxied from specified domain. Found: {} - Expected: {}",
            hostname, PROXY_FROM_DOMAIN
        ));
    }

    try_get_cached_response(request)
        .await
        .and_then(bytes_to_body)
}

struct CachedResponse {
    request: http::Request<Bytes>,
    response: http::Response<Bytes>,
    cached_at: chrono::DateTime<chrono::Utc>,
}

impl CachedResponse {
    fn new(request: http::Request<Bytes>, response: http::Response<Bytes>) -> Self {
        Self {
            request,
            response,
            cached_at: chrono::Utc::now(),
        }
    }
}

#[tracing::instrument(skip_all)]
async fn try_get_cached_response(
    request: Request<Body>,
) -> miette::Result<http::Response<Bytes>, String> {
    info!("Request headers: {:?}", request.headers());
    let url = request.uri().clone();

    {
        let cache = CACHE.lock().unwrap();
        if let Some(cached) = cache.get(&(request.method().clone(), url.clone())) {
            let policy = CachePolicy::new(&cached.request, &cached.response);
            match policy.before_request(&request, SystemTime::now()) {
                BeforeRequest::Fresh(_) => {
                    info!("Cache hit for: {}", url);
                    return Ok(cached.response.clone());
                }
                BeforeRequest::Stale {
                    request: new_request,
                    matches,
                } => {
                    info!(
                        matches,
                        headers = ?new_request.headers(),
                        "Cache hit but response is stale: {}",
                        url
                    );
                }
            }
        }
    }

    warn!("Cache miss for: {}", url);
    let origin_url = Uri::builder()
        .scheme("http")
        .authority(PROXY_ORIGIN_DOMAIN)
        .path_and_query(url.path_and_query().map(|pq| pq.path()).unwrap_or("/"))
        .build()
        .map_err(|e| format!("Failed to build url: {}", e))?;
    let request = body_to_bytes(request).await?;

    let client = reqwest::Client::new();
    let origin_response = client
        .request(
            ReqMethod::from_str(request.method().as_str()).unwrap(),
            origin_url.to_string(),
        )
        .headers(map_to_reqwest_headers(request.headers().clone()))
        .body(request.body().clone())
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let response = {
        let response = into_axum_response(origin_response).await?;
        let mut cache = CACHE.lock().unwrap();
        cache.insert(
            (request.method().clone(), url),
            CachedResponse::new(request, response.clone()),
        );

        response
    };

    Ok(response)
}
