use std::{borrow::Cow, marker::PhantomData};

use anyhow::Context;
use axum::{Json, http::StatusCode, response::IntoResponse};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use redis::{AsyncTypedCommands, aio::MultiplexedConnection};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::{
    database::try_end_transaction, extractors::Cbor, impl_deref,
};

pub mod manager;
pub mod shared;
pub mod user;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request<T> {
    data: T,
    nonce: Option<Ulid>,
}

impl_deref!(impl<T> ref Request<T> => T = .data);

impl<T: Send + Sync> Request<T> {
    async fn verified<S>(
        self,
        cache_conn: &mut MultiplexedConnection,
    ) -> ApiResult<T, S> {
        let Some(nonce) = self.nonce else {
            return Ok(self.data);
        };

        verify_nonce(cache_conn, nonce).await?;

        Ok(self.data)
    }
}

// TODO: RFC9457
#[derive(Debug, Serialize, Deserialize)]
pub enum Response<'msg, T> {
    Ok(T),
    Err { code: ErrCode, msg: Cow<'msg, str> },
}

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    IntoPrimitive,
    TryFromPrimitive,
)]
#[serde(try_from = "u64", into = "u64")]
#[repr(u64)]
pub enum ErrCode {
    Unknown = 1,
    // client error
    InvalidToken = 4000,
    UsedNonce,
    InvalidCredential,
    InsufficientPermission,
    RequireSudo,
    BadParam,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError<T> {
    #[error("{0}")]
    Unknown(#[from] anyhow::Error),
    #[error("invalid token: {0}")]
    InvalidToken(&'static str),
    #[error("used nonce")]
    UsedNonce,
    #[error("invalid credential: {0}")]
    InvalidCredential(&'static str),
    #[error("insufficient permission: {0}")]
    InsufficientPermission(&'static str),
    #[error("require sudo")]
    RequireSudo,
    #[error("bad parameter: {detail}")]
    BadParam { msg: Box<str>, detail: Box<str> },
    #[error("response serialization type marker")]
    __(PhantomData<T>),
}

impl<T> ApiError<T> {
    #[expect(clippy::cognitive_complexity)]
    pub fn into_api_response(
        self,
    ) -> (StatusCode, Response<'static, ()>) {
        match self {
            Self::Unknown(err) => {
                tracing::error!(
                    "unexpected error during request handling: {err:?}"
                );

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Response::Err {
                        code: ErrCode::Unknown,
                        msg:
                            "unknown error, please contact administrator"
                                .into(),
                    },
                )
            }
            Self::InvalidToken(msg) => {
                tracing::info!("rejecting invalid token: {msg}");

                (
                    StatusCode::UNAUTHORIZED,
                    Response::Err {
                        code: ErrCode::InvalidToken,
                        msg: "please login first".into(),
                    },
                )
            }
            Self::UsedNonce => {
                tracing::info!("rejecting used nonce");

                (
                    StatusCode::BAD_REQUEST,
                    Response::Err {
                        code: ErrCode::UsedNonce,
                        msg: "used nonce".into(),
                    },
                )
            }
            Self::InvalidCredential(msg) => {
                tracing::info!("rejecting invalid credential: {msg}");

                (
                    StatusCode::UNAUTHORIZED,
                    Response::Err {
                        code: ErrCode::InvalidCredential,
                        msg: "invalid credential".into(),
                    },
                )
            }
            Self::InsufficientPermission(msg) => {
                tracing::info!(
                    "rejecting insufficient permission: {msg}"
                );

                (
                    StatusCode::FORBIDDEN,
                    Response::Err {
                        code: ErrCode::InsufficientPermission,
                        msg: "you are not allowed to do this".into(),
                    },
                )
            }
            Self::RequireSudo => {
                tracing::info!("rejecting non sudo access");

                (
                    StatusCode::FORBIDDEN,
                    Response::Err {
                        code: ErrCode::RequireSudo,
                        msg: "enter sudo mode first".into(),
                    },
                )
            }
            Self::BadParam { msg, detail } => {
                tracing::info!("rejecting bad parameter: {detail}");

                (
                    StatusCode::BAD_REQUEST,
                    Response::Err {
                        code: ErrCode::BadParam,
                        msg: msg.to_string().into(),
                    },
                )
            }
            Self::__(_) => unreachable!(),
        }
    }
}

pub type ApiResult<T, S> = Result<T, ApiError<S>>;

#[derive(Debug)]
pub struct ToJson();
#[derive(Debug)]
pub struct ToCbor();

impl IntoResponse for ApiError<ToJson> {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = self.into_api_response();
        (status, Json(body)).into_response()
    }
}

impl IntoResponse for ApiError<ToCbor> {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = self.into_api_response();
        (status, Cbor(body)).into_response()
    }
}

#[macro_export]
macro_rules! api_begin_transaction {
    ($db:expr, $conn:ident, $trans:ident, $stmt:ident) => {
        let mut $conn = ::anyhow::Context::context(
            $db.acquire().await,
            "db connection acquire",
        )?;

        let mut $trans = ::anyhow::Context::context(
            ::sqlx::Connection::begin_with(
                &mut *$conn,
                $crate::database::BeginStmt::$stmt,
            )
            .await,
            "transaction begin",
        )?;
    };
}

async fn spawn_await<Fut>(fut: Fut) -> anyhow::Result<Fut::Output>
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    tokio::task::spawn(fut).await.context("join tokio task")
}

async fn end_transaction<DB, R, E, S>(
    result: Result<R, E>,
    trans: sqlx::Transaction<'_, DB>,
) -> ApiResult<R, S>
where
    DB: sqlx::Database,
    E: Into<ApiError<S>>,
{
    let res = try_end_transaction(result, trans)
        .await
        .context("transaction end");

    match res {
        Ok(ok) => ok.map_err(Into::into),
        Err(err) => Err(ApiError::Unknown(err)),
    }
}

async fn verify_nonce<S>(
    cache_conn: &mut MultiplexedConnection,
    nonce: Ulid,
) -> ApiResult<(), S> {
    let not_exists = cache_conn
        .set_nx(format!("nonce:{nonce}"), "")
        .await
        .context("failed to set_nx nonce in cache")?;

    if not_exists {
        Ok(())
    } else {
        Err(ApiError::UsedNonce)
    }
}

#[macro_export]
macro_rules! api_param_assert {
    ($expr:expr, $msg:literal) => {
        if !($expr) {
            return Err($crate::api::ApiError::BadParam {
                msg: $msg.into(),
                detail: stringify!($expr).into(),
            });
        }
    };
    ($expr:expr) => {
        api_param_assert!($expr, "bad param")
    };
}

#[macro_export]
macro_rules! api_assert {
    ($expr:expr, $msg:literal) => {
        if !($expr) {
            return Err($crate::api::ApiError::Unknown(
                ::anyhow::anyhow!("{}: {}", $msg, stringify!($expr)),
            ));
        }
    };
    ($expr:expr) => {
        api_assert!($expr, "assertion failed")
    };
}
