use axum::{
    body::{Body, Bytes},
    http::{self, response::Builder, HeaderMap, HeaderName, HeaderValue},
    response::Response,
};
use miette::IntoDiagnostic;
use reqwest::header::{
    HeaderMap as ReqHeaderMap, HeaderName as ReqHeaderName, HeaderValue as ReqHeaderValue,
};

pub fn add_headers(mut response_builder: Builder, headers: &HeaderMap) -> Builder {
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
    let mut new_response = http::Response::builder().status(response.status());
    let new_response = add_headers(new_response, response.headers());

    new_response
        .body(Body::from(response.body().clone()))
        .map_err(|e| format!("Failed to convert bytes to body: {}", e))
}

pub async fn into_axum_response(
    response: reqwest::Response,
) -> miette::Result<Response<Bytes>, String> {
    let mut response_builder = Response::builder().status(response.status().as_u16());
    response_builder.headers_mut().map(|headers| {
        headers.extend(response.headers().into_iter().map(|(name, value)| {
            // TODO: change unwrap to something better
            let name = HeaderName::from_bytes(name.as_ref()).unwrap();
            let value = HeaderValue::from_bytes(value.as_ref()).unwrap();
            (name, value)
        }))
    });

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
