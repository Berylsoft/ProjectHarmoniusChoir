#![warn(missing_debug_implementations)]
#![warn(clippy::pedantic, clippy::nursery)]
// #![clippy::too_many_line_threshold = 60]
#![allow(clippy::default_trait_access)]

use std::{
    env::{self, VarError},
    ffi::OsStr,
    fmt::Debug,
    fs,
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{Response, StatusCode, header},
    routing,
};
use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use cookie::{Cookie, SameSite};
use database::Database;
use ed25519_dalek::{
    SigningKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey},
};
use extractors::{Cookies, Token};
use serde::{Deserialize, Serialize};
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
use tracing_subscriber::EnvFilter;
use ulid::Ulid;

mod database;
mod extractors;
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
    key: SigningKey,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserToken {
    pub uid: i64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expired: DateTime<Utc>,
}

async fn test(
    state: State<ServerState>,
    token: Token<UserToken>,
    _req: Request,
) -> Result<Response<Body>, Response<Body>> {
    tracing::info!("{:?}", token);

    let mut c = Cookie::new(
        "token",
        SignedData::sign(
            UserToken {
                uid: 114,
                expired: Utc::now(),
            },
            &state.key,
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
        .header(header::SET_COOKIE, c.encoded().to_string())
        .body(Body::empty())
        .unwrap())
}

fn router(state: ServerState) -> Router {
    Router::new()
        .route("/", routing::get(test))
        .layer((
            SetRequestIdLayer::x_request_id(ServerMakeRequestId),
            TraceLayer::new_for_http().make_span_with(
                DefaultMakeSpan::new().include_headers(true),
            ),
            TimeoutLayer::new(Duration::from_secs(15)),
            PropagateRequestIdLayer::x_request_id(),
        ))
        .with_state(state)
}

fn init_env() {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .event_format(tracing_subscriber::fmt::format().pretty())
        .with_env_filter(EnvFilter::from_default_env())
        // .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();
}

fn var_optional(
    key: impl AsRef<OsStr>,
) -> Result<Option<String>, VarError> {
    match env::var(key) {
        Ok(var) => Ok(Some(var)),
        Err(VarError::NotPresent) => Ok(None),
        Err(err) => Err(err),
    }
}

fn get_or_init_signing_key() -> anyhow::Result<SigningKey> {
    let env_key = var_optional("SIGNING_KEY")
        .context("failed to get SIGNING_KEY env")?
        .map(|it| BASE64_URL_SAFE_NO_PAD.decode(it))
        .transpose()
        .context("failed to decode SIGNING_KEY as base64")?
        .map(|it| SigningKey::from_pkcs8_der(&it))
        .transpose()
        .context("failed to parse SIGNING_KEY as pkcs8_der")?;

    if let Some(key) = env_key {
        return Ok(key);
    }

    let file_key = fs::exists("./signing_key.der")
        .context("failed to check if signing_key.der exists")?
        .then(|| fs::read("./signing_key.der"))
        .transpose()
        .context("failed to read ./signing_key.der")?
        .map(|it| SigningKey::from_pkcs8_der(&it))
        .transpose()
        .context("failed to parse ./signing_key.der")?;

    if let Some(key) = file_key {
        return Ok(key);
    }

    let key = SigningKey::generate(&mut rand_core::OsRng);

    let der = key.to_pkcs8_der().expect("expect encoding success");
    der.write_der_file("./signing_key.der")
        .context("failed to write ./signing_key.der")?;
    let der_base64 = BASE64_URL_SAFE_NO_PAD.encode(der.as_bytes());
    fs::write("./signing_key.der.base64", der_base64)
        .context("failed to write ./signing_key.der.base64")?;

    Ok(key)
}

async fn run() -> anyhow::Result<()> {
    init_env();

    info!("initializing");
    let state = ServerState {
        key: get_or_init_signing_key()
            .context("failed to get_or_init signingkey")?,
        db: Database::init("sqlite://data/database.db?mode=rwc")
            .await
            .context("failed to initialize database")?,
    };

    let host = var_optional("HOST")
        .context("failed to get HOST env")?
        .unwrap_or_else(|| "0.0.0.0:8081".to_string());

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
