#![warn(missing_debug_implementations)]
#![warn(clippy::pedantic, clippy::nursery)]
// #![clippy::too_many_line_threshold = 60]
#![allow(clippy::default_trait_access)]

use std::{
    env::{self, VarError},
    time::Duration,
};

use anyhow::Context;
use axum::{Router, response::IntoResponse, routing};
use tokio::net::TcpListener;
use tower_http::{
    request_id::{
        MakeRequestId, PropagateRequestIdLayer, RequestId,
        SetRequestIdLayer,
    },
    timeout::TimeoutLayer,
    trace::{DefaultMakeSpan, TraceLayer},
};
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};
use ulid::Ulid;

pub async fn test() -> impl IntoResponse {
    "test"
}

#[derive(Debug, Clone, Copy)]
struct ServerMakeRequestId;

impl MakeRequestId for ServerMakeRequestId {
    fn make_request_id<B>(
        &mut self,
        _request: &axum::http::Request<B>,
    ) -> Option<RequestId> {
        Some(RequestId::new(Ulid::new().to_string().parse().unwrap()))
    }
}

#[derive(Clone)]
pub struct ServerState {}

pub fn router() -> Router {
    Router::new()
        .with_state(0_u32)
        .route("/", routing::get(test))
        .layer((
            SetRequestIdLayer::x_request_id(ServerMakeRequestId),
            TraceLayer::new_for_http().make_span_with(
                DefaultMakeSpan::new().include_headers(true),
            ),
            TimeoutLayer::new(Duration::from_secs(15)),
            PropagateRequestIdLayer::x_request_id(),
        ))
}

async fn run() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();

    let host = env::var("HOST")
        .or_else(|err| {
            if matches!(err, VarError::NotPresent) {
                Ok("0.0.0.0:8081".to_string())
            } else {
                Err(err)
            }
        })
        .context("failed to get HOST env")?;

    let listener = TcpListener::bind(&host)
        .await
        .context("failed to bind listener")?;

    info!("listening on {host}");

    axum::serve(listener, router())
        .await
        .context("failed to serve")?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(run())
}
