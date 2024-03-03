use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, HeaderName, HeaderValue},
    response::{IntoResponse, Response},
};
use miette::IntoDiagnostic;
use reqwest::header::{
    HeaderMap as ReqHeaderMap, HeaderName as ReqHeaderName, HeaderValue as ReqHeaderValue,
};

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

pub async fn response_body_to_bytes(response: reqwest::Response) -> miette::Result<Bytes, String> {
    response
        .bytes()
        .await
        .into_diagnostic()
        .map_err(|e| format!("failed to get bytes from response: {}", e))
}

pub async fn into_axum_response(
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
