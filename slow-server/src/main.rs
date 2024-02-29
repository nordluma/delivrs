use std::time::Duration;

use actix_web::{web, App, HttpServer};
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

async fn index() -> actix_web::Result<Markup> {
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

async fn fast() -> actix_web::Result<Markup> {
    let now = Utc::now();
    tokio::time::sleep(Duration::from_secs(1)).await;

    Ok(html!(
        html {
            h1 { r#""Fast" endpoint"#}
            p { "Current time: " (now) }
        }
    ))
}

async fn slow() -> actix_web::Result<Markup> {
    let now = Utc::now();
    tokio::time::sleep(Duration::from_secs(5)).await;

    Ok(html!(
        html {
            h1 { r#""Slow" endpoint"#}
            p { "Current time: " (now) }
        }
    ))
}