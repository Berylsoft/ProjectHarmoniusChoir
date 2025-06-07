#![warn(missing_debug_implementations)]
#![warn(clippy::pedantic, clippy::nursery)]
// #![clippy::too_many_line_threshold = 60]
#![allow(clippy::default_trait_access)]
#![allow(warnings)]

use std::{
    env::{self, VarError},
    fmt::Display,
    io::Cursor,
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{
    Router,
    body::Body,
    extract::{OptionalFromRequestParts, Request},
    http::{Response, StatusCode, request},
    routing,
};
use base64::Engine;
use chrono::{DateTime, Utc};
use cookie::{
    Cookie, CookieJar, Expiration, SameSite,
    time::{Date, OffsetDateTime},
};
use database::Database;
use ed25519_dalek::{
    SIGNATURE_LENGTH, Signature, SignatureError, SigningKey,
    VerifyingKey, ed25519::signature::Signer,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use signing::SignedData;
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
use utils::to_cbor;

mod database;
mod signing;
mod utils;

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
struct ServerState {
    db: Database,
}

fn map_err_warn_bad_request<'log, E: Display>(
    log: &'log str,
) -> impl (FnOnce(E) -> Response<Body>) + use<'log, E> {
    return move |err| {
        tracing::warn!("{log}: {err}");
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap()
    };
}

#[derive(Debug, Default)]
struct Cookies(pub CookieJar);

impl_deref!(mut Cookies => CookieJar = .0);

impl<S> OptionalFromRequestParts<S> for Cookies
where
    S: Send + Sync,
{
    type Rejection = Response<Body>;

    async fn from_request_parts(
        parts: &mut request::Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        parts
            .headers
            .get("Cookie")
            .map(|it| {
                it.to_str().map_err(map_err_warn_bad_request(
                    "invalid cookie header value",
                ))
            })
            .transpose()?
            .map(|it| {
                let mut jar = CookieJar::new();
                Cookie::split_parse_encoded(it)
                    .try_for_each(|it| {
                        jar.add_original(it?.into_owned());

                        Result::<_, cookie::ParseError>::Ok(())
                    })
                    .map(|()| Self(jar))
                    .map_err(map_err_warn_bad_request("invalid cookie"))
            })
            .transpose()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Token {
    pub uid: i64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expired: DateTime<Utc>,
}

async fn test(
    cookies: Option<Cookies>,
    req: Request,
) -> Result<Response<Body>, Response<Body>> {
    let mut jar = cookies.unwrap_or_default();
    dbg!(jar.get("token"));

    let mut c = Cookie::new(
        "token",
        SignedData::sign(
            Token {
                uid: 114,
                expired: Utc::now(),
            },
            &ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng),
        )
        .to_encoded()
        .to_string(),
    );
    c.set_max_age(cookie::time::Duration::days(365));
    c.set_secure(true);
    c.set_http_only(true);
    c.set_same_site(SameSite::Strict);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Set-Cookie", c.encoded().to_string())
        .body(Body::empty())
        .unwrap())
}

fn router(state: ServerState) -> Router {
    Router::new()
        .with_state(state)
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

fn init_env() {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();
}

async fn run() -> anyhow::Result<()> {
    init_env();

    let state = ServerState {
        db: Database::init("sqlite://data/database.db?mode=rwc")
            .await
            .context("failed to initialize database")?,
    };

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

    axum::serve(listener, router(state))
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
