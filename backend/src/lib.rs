#![warn(missing_debug_implementations)]
#![warn(clippy::pedantic, clippy::nursery)]
// #![clippy::too_many_line_threshold = 60]
#![allow(clippy::default_trait_access)]

use std::{
    collections::HashMap,
    env::{self, VarError},
    ffi::OsStr,
    fmt::Debug,
    fs,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::operation::head_bucket::HeadBucketError;
use axum::{Router, routing};
use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use database::Database;
use ed25519_dalek::{
    SigningKey,
    pkcs8::{DecodePrivateKey, EncodePrivateKey},
};
use mimalloc::MiMalloc;
use redis::aio::MultiplexedConnection;
use tokio::{
    net::TcpListener,
    sync::{Mutex, oneshot},
};
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

use crate::api::{
    manager::{
        self, acquire_sudo, create_manager, create_project,
        init_root_if_not_exists, login, project_manager_edit,
    },
    user::{
        self, join_project, revoke_all_tokens, wechat_login_or_register,
    },
};

pub mod api;
pub mod database;
mod extractors;
pub mod signing;
mod utils;

#[global_allocator]
static GLOBAL_ALLOCATOR: MiMalloc = MiMalloc;

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

#[derive(Debug, Clone)]
pub struct S3 {
    client: aws_sdk_s3::Client,
    bucket: Arc<str>,
}

impl S3 {
    pub fn new(
        client: aws_sdk_s3::Client,
        bucket: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            client,
            bucket: bucket.into(),
        }
    }

    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }
}

impl_deref!(ref S3 => aws_sdk_s3::Client = .client);

#[derive(Debug)]
pub struct PendingJob {
    pub cancel: Option<oneshot::Sender<()>>,
    pub wait: Option<oneshot::Receiver<()>>,
}

pub type PendingJobs = Arc<Mutex<HashMap<i64, PendingJob>>>;

#[derive(Debug, Clone)]
pub struct ServerState {
    pub key: SigningKey,
    pub db: Database,
    pub cache: MultiplexedConnection,
    pub s3: S3,
    pub pending_jobs: PendingJobs,
}

impl ServerState {
    #[must_use]
    pub fn new(
        key: SigningKey,
        db: Database,
        cache: MultiplexedConnection,
        s3: S3,
    ) -> Self {
        Self {
            key,
            db,
            cache,
            s3,
            pending_jobs: Default::default(),
        }
    }
}

#[expect(clippy::too_many_lines)]
pub fn router<MakeReqId>(
    state: ServerState,
    make_req_id: MakeReqId,
) -> Router
where
    MakeReqId: MakeRequestId + Send + Sync + Clone + 'static,
{
    Router::new()
        .route(
            "/api/user/wechat_login_or_register",
            routing::post(wechat_login_or_register::router),
        )
        .route(
            "/api/user/revoke_all_tokens",
            routing::post(revoke_all_tokens::router),
        )
        .route(
            "/api/user/get_info",
            routing::post(user::get_info::router),
        )
        .route(
            "/api/user/list_projects",
            routing::post(user::list_projects::router),
        )
        .route(
            "/api/user/join_project",
            routing::post(join_project::router),
        )
        .route(
            "/api/user/project_info",
            routing::post(user::project_info::router),
        )
        .route(
            "/api/user/upload_file",
            routing::post(user::upload_file::router),
        )
        .route(
            "/api/user/list_pending_files",
            routing::post(user::list_pending_files::router),
        )
        .route(
            "/api/user/delete_file",
            routing::post(user::delete_file::router),
        )
        .route(
            "/api/user/pre_submit",
            routing::post(user::pre_submit::router),
        )
        .route(
            "/api/user/agree_nda",
            routing::post(user::agree_nda::router),
        )
        .route(
            "/api/user/get_attachment",
            routing::post(user::get_attachment::router),
        )
        .route("/api/user/submit", routing::post(user::submit::router))
        .route("/api/manager/login", routing::post(login::router))
        .route(
            "/api/manager/acquire_sudo",
            routing::post(acquire_sudo::router),
        )
        .route(
            "/api/manager/list_projects",
            routing::post(manager::list_projects::router),
        )
        .route(
            "/api/manager/list_project_users",
            routing::post(manager::list_project_users::router),
        )
        .route(
            "/api/manager/pre_submit_info",
            routing::post(manager::pre_submit_info::router),
        )
        .route(
            "/api/manager/get_file",
            routing::post(manager::get_file::router),
        )
        .route(
            "/api/manager/pre_submit_review",
            routing::post(manager::pre_submit_review::router),
        )
        .route(
            "/api/manager/submit_info",
            routing::post(manager::submit_info::router),
        )
        .route(
            "/api/manager/submit_review",
            routing::post(manager::submit_review::router),
        )
        .route(
            "/api/manager/upload_file",
            routing::post(manager::upload_file::router),
        )
        .route(
            "/api/manager/list_pending_files",
            routing::post(manager::list_pending_files::router),
        )
        .route(
            "/api/manager/delete_file",
            routing::post(manager::delete_file::router),
        )
        .route(
            "/api/manager/master",
            routing::post(manager::master::router),
        )
        .route(
            "/api/manager/master_info",
            routing::post(manager::master_info::router),
        )
        .route(
            "/api/manager/bundle_job",
            routing::post(manager::bundle_job::router),
        )
        .route(
            "/api/manager/root/create_project",
            routing::post(create_project::router),
        )
        .route(
            "/api/manager/root/create_manager",
            routing::post(create_manager::router),
        )
        .route(
            "/api/manager/root/list_managers",
            routing::post(manager::list_managers::router),
        )
        .route(
            "/api/manager/root/project_manager_edit",
            routing::post(project_manager_edit::router),
        )
        .layer((
            SetRequestIdLayer::x_request_id(make_req_id),
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

/// # Errors
/// invalid env or redis error
pub async fn init_cache() -> anyhow::Result<MultiplexedConnection> {
    info!("connecting cache");
    let redis_url = var_optional("REDIS_DB")
        .context("failed to get REDIS_DB env")?
        .unwrap_or_else(|| "redis://localhost".to_string());
    let client =
        redis::Client::open(redis_url).context("invalid redis db url")?;
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .context("failed to get redix connection")?;

    redis::cmd("CONFIG")
        .arg("SET")
        .arg("maxmemory")
        .arg("1gb")
        .exec_async(&mut conn)
        .await
        .context("failed to set maxmemory")?;

    redis::cmd("CONFIG")
        .arg("SET")
        .arg("maxmemory-policy")
        .arg("allkeys-lru")
        .exec_async(&mut conn)
        .await
        .context("failed to set maxmemory-policy")?;

    redis::cmd("CONFIG")
        .arg("SET")
        .arg("save")
        .arg("")
        .exec_async(&mut conn)
        .await
        .context("failed to set save")?;

    Ok(conn)
}

/// # Errors
/// invalid env or s3 error
pub async fn init_s3() -> anyhow::Result<S3> {
    let aws_config =
        aws_config::load_defaults(BehaviorVersion::latest()).await;
    let mut s3_config_builder =
        aws_sdk_s3::config::Builder::from(&aws_config);

    let force_path_style = var_optional("S3_FORCE_PATH_STYLE")
        .context("failed to get S3_FORCE_PATH_STYLE env")?
        .as_deref()
        .map(str::parse::<bool>)
        .transpose()
        .context("failed to parse S3_FORCE_PATH_STYLE as bool")?;
    s3_config_builder.set_force_path_style(force_path_style);

    let s3_config = s3_config_builder.build();

    let client = aws_sdk_s3::Client::from_conf(s3_config);

    let bucket = var_optional("S3_BUCKET")
        .context("failed to get S3_BUCKET env")?
        .context("expect S3_BUCKET")?;

    let init_bucket = var_optional("S3_INIT_BUCKET_IF_NOT_EXISTS")
        .context("failed to get S3_INIT_BUCKET_IF_NOT_EXISTS env")?
        .is_some_and(|it| it == "true");

    if init_bucket {
        let res = client.head_bucket().bucket(&bucket).send().await;
        if let Err(err) = &res
            && let Some(HeadBucketError::NotFound(_)) =
                err.as_service_error()
        {
            tracing::info!("s3 bucket not exists, creating");
            client.create_bucket().bucket(&bucket).send().await?;
        } else {
            let _ = res.context("create bucket when not exists")?;
        }
    }

    Ok(S3::new(client, bucket))
}

async fn initialize_server_state() -> anyhow::Result<ServerState> {
    let key = get_or_init_signing_key()
        .context("failed to get_or_init signingkey")?;

    let db = Database::init(
        var_optional("SQL_DB")
            .context("failed to get SQL_DB env")?
            .unwrap_or_else(|| {
                "sqlite://data/database.db?mode=rwc".to_string()
            }),
    )
    .await
    .context("failed to initialize database")?;

    let cache = init_cache().await.context("failed to init cache")?;

    let s3 = init_s3().await.context("init s3")?;

    let state = ServerState {
        key,
        db,
        cache,
        s3,
        pending_jobs: Default::default(),
    };

    let default_password = init_root_if_not_exists(&state)
        .await
        .context("init_root_if_not_exists")?;
    if let Some(default_password) = default_password {
        info!("default root password: {default_password}");
    }

    Ok(state)
}

async fn cancel_jobs(jobs: &PendingJobs) {
    // expect the job remove the entry before send to wait
    loop {
        let mut jobs = jobs.lock().await;
        let Some((id, job)) = jobs.iter_mut().next() else {
            break;
        };
        info!("canceling job {id}");

        if let Some(cancel) = job.cancel.take() {
            let _ = cancel.send(());
        }

        let Some(wait) = job.wait.take() else {
            continue;
        };
        drop(jobs);

        let _ = wait.await;
    }
}

/// # Errors
/// Fatal errors
pub async fn run() -> anyhow::Result<()> {
    init_env();

    info!("initializing");
    let state = initialize_server_state()
        .await
        .context("failed to initialize server state")?;

    let host = var_optional("HOST")
        .context("failed to get HOST env")?
        .unwrap_or_else(|| "0.0.0.0:8081".to_string());

    let listener = TcpListener::bind(&host)
        .await
        .context("failed to bind listener")?;

    info!("listening on {host}");

    axum::serve(listener, router(state.clone(), ServerMakeRequestId))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("failed to serve")?;

    cancel_jobs(&state.pending_jobs).await;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("tokio::signal::ctrl_c()");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .expect("tokio::signal::unix::signal(SignalKind::terminate())")
        .recv()
        .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown");
}

/// # Errors
/// Fatal errors
pub fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(run())
}
