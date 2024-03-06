use std::time::Duration;

use actix_web::{
    http::header::{CacheControl, CacheDirective, ContentType},
    web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use chrono::Utc;
use maud::{html, Markup};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port = 8080;
    HttpServer::new(|| {
        App::new()
            .route("/", web::to(index))
            .route("/fast", web::to(fast))
            .route("/slow", web::to(slow))
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}

async fn index(request: HttpRequest) -> actix_web::Result<Markup> {
    println!("->> HANDLER - index: {:?}", request);
    Ok(html!(
        html {
            body {
                h1 { "A slow server "}
                p {
                    "The slow server has two endpoint (excluding index)."
                }
                ul {
                    li { a href="/fast" { "/fast" } " responds after one second" }
                    li { a href="/slow" { "/slow" } " responds after five seconds" }
                }
            }
        }
    ))
}

async fn fast(request: HttpRequest) -> impl Responder {
    println!("->> HANDLER - fast: {:?}", request);
    let now = Utc::now();
    tokio::time::sleep(Duration::from_secs(1)).await;

    ok_with_cache_headers(
        html!(
            html {
                h1 { r#""Fast" endpoint"# }
                p { "Current time: " (now) }
            }
        )
        .into(),
    )
}

async fn slow(request: HttpRequest) -> impl Responder {
    println!("->> HANDLER - slow: {:?}", request);
    let now = Utc::now();
    tokio::time::sleep(Duration::from_secs(5)).await;

    ok_with_cache_headers(
        html!(
            html {
                h1 { r#""Slow" endpoint"# }
                p { "Current time: " (now) }
            }
        )
        .into(),
    )
}

fn ok_with_cache_headers(body: String) -> HttpResponse {
    let response = HttpResponse::Ok()
        .insert_header(CacheControl(vec![CacheDirective::MaxAge(60)]))
        .content_type(ContentType::html())
        .body(body);

    println!("->> RESPONSE - {:#?}", response);

    response
}
