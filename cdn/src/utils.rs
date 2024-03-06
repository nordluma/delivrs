use axum::{
    body::{to_bytes, Body, Bytes},
    http::{self, response::Builder, HeaderMap, HeaderName, HeaderValue},
    response::Response,
};
use miette::IntoDiagnostic;
use reqwest::header::{
    HeaderMap as ReqHeaderMap, HeaderName as ReqHeaderName, HeaderValue as ReqHeaderValue,
};

pub fn response_with_headers(mut response_builder: Builder, headers: &HeaderMap) -> Builder {
    for (key, value) in headers {
        response_builder = response_builder.header(key, value);
    }

    response_builder
}

pub fn map_to_reqwest_headers(headers: HeaderMap) -> ReqHeaderMap {
    let mut reqwest_headers = ReqHeaderMap::with_capacity(headers.len());
    reqwest_headers.extend(headers.into_iter().map(|(name, value)| {
        // TODO: change unwrap to something better
        let name = ReqHeaderName::from_bytes(name.unwrap().as_ref()).unwrap();
        let value = ReqHeaderValue::from_bytes(value.as_ref()).unwrap();

        (name, value)
    }));

    reqwest_headers
}

pub fn bytes_to_body(
    response: http::Response<Bytes>,
) -> miette::Result<http::Response<Body>, String> {
    let new_response = response_with_headers(
        http::Response::builder().status(response.status()),
        response.headers(),
    );

    new_response
        .body(Body::from(response.body().clone()))
        .map_err(|e| format!("Failed to convert bytes to body: {}", e))
}

pub async fn body_to_bytes(
    request: http::Request<Body>,
) -> miette::Result<http::Request<Bytes>, String> {
    let (parts, body) = request.into_parts();
    let body_bytes = to_bytes(body, usize::MAX)
        .await
        .map_err(|e| format!("Failed to convert body to bytes: {}", e))?;

    Ok(http::Request::from_parts(parts, body_bytes))
}

pub async fn into_axum_response(
    response: reqwest::Response,
) -> miette::Result<Response<Bytes>, String> {
    let mut response_builder = Response::builder().status(response.status().as_u16());
    if let Some(headers) = response_builder.headers_mut() {
        headers.extend(response.headers().into_iter().map(|(name, value)| {
            // TODO: change unwrap to something better
            let name = HeaderName::from_bytes(name.as_ref()).unwrap();
            let value = HeaderValue::from_bytes(value.as_ref()).unwrap();

            (name, value)
        }))
    };

    let response = response_builder
        .body(
            response
                .bytes()
                .await
                .into_diagnostic()
                .map_err(|e| format!("failed to get bytes from response: {}", e))?,
        )
        .map_err(|e| format!("failed to set response body: {}", e))?;

    Ok(response)
}
