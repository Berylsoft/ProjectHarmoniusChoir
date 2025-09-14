use std::{borrow::Cow, marker::PhantomData};

use anyhow::Context;
use axum::{Json, http::StatusCode, response::IntoResponse};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use redis::{AsyncTypedCommands, aio::MultiplexedConnection};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::{
    database::try_end_transaction,
    extractors::Cbor,
    impl_deref,
    wechat::{self, Wechat},
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
    NotFound,
    InvalidStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError<T> {
    #[error("{0}")]
    Unknown(#[from] anyhow::Error),
    #[error("invalid token: {0}")]
    InvalidToken(&'static str),
    #[error("used nonce")]
    UsedNonce,
    #[error("insufficient permission: {0}")]
    InsufficientPermission(&'static str),
    #[error("require sudo")]
    RequireSudo,
    #[error("bad parameter: {detail}")]
    BadParam { msg: Box<str>, detail: Box<str> },
    #[error("not found: {detail}")]
    NotFound { msg: Box<str>, detail: Box<str> },
    #[error("invalid status: {detail}")]
    InvalidStatus { msg: Box<str>, detail: Box<str> },
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
                tracing::info!(
                    "rejecting bad parameter: {msg}({detail})"
                );

                (
                    StatusCode::BAD_REQUEST,
                    Response::Err {
                        code: ErrCode::BadParam,
                        msg: msg.to_string().into(),
                    },
                )
            }
            Self::NotFound { msg, detail } => {
                tracing::info!(
                    "requested resource not found: {msg}({detail})"
                );

                (
                    StatusCode::NOT_FOUND,
                    Response::Err {
                        code: ErrCode::NotFound,
                        msg: msg.to_string().into(),
                    },
                )
            }
            Self::InvalidStatus { msg, detail } => {
                tracing::info!(
                    "rejecting invalid status: {msg}({detail})"
                );

                (
                    StatusCode::CONFLICT,
                    Response::Err {
                        code: ErrCode::InvalidStatus,
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

#[must_use]
pub fn is_valid_name(name: &str) -> bool {
    const MAX_LENGTH: usize = 20;

    if name.is_empty() {
        return false;
    }

    if name.len() > MAX_LENGTH * 4 {
        return false;
    }

    let mut cnt = 0;
    for ch in name.chars() {
        cnt += 1;
        if cnt > MAX_LENGTH {
            return false;
        }

        // check for Cc,Cs,Co
        // Cs is not allowed in UTF-8, so checked by rust
        // Cc:
        if ch.is_control() {
            return false;
        }
        // Co:
        if matches!(ch, '\u{E000}'..='\u{F8FF}')
            | matches!(ch, '\u{F_0000}'..='\u{F_FFFD}')
            | matches!(ch, '\u{10_0000}'..='\u{10_FFFD}')
        {
            return false;
        }
    }

    true
}
/// failed assert indicate a client fault
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

/// failed assert indicate a server fault
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

/// indicate a server fault
#[macro_export]
macro_rules! api_bail {
    ($($tt:tt)*) => {
        return Err($crate::api::ApiError::Unknown(
            ::anyhow::anyhow!($($tt)*),
        ))
    };
}

/// indicate a client fault
#[macro_export]
macro_rules! api_bail_not_found {
    ($msg:expr, $detail:expr) => {
        return Err($crate::api::ApiError::NotFound {
            msg: $msg.into(),
            detail: $detail.into(),
        })
    };
    ($msg:expr) => {
        api_bail_not_found!($msg, $msg)
    };
}

/// indicate a client fault
#[macro_export]
macro_rules! api_bail_status {
    ($msg:expr, $detail:expr) => {
        return Err($crate::api::ApiError::InvalidStatus {
            msg: $msg.into(),
            detail: $detail.into(),
        })
    };
    ($msg:expr) => {
        api_bail_status!($msg, $msg)
    };
}

async fn get_openid_by_jscode<S>(
    wechat: &dyn Wechat,
    jscode: &str,
) -> ApiResult<Box<str>, S> {
    let login_res = wechat.jscode2session(jscode.into()).await;
    match login_res {
        Ok(openid) => Ok(openid),
        Err(err) => match err {
            wechat::Error::InvalidJscode(msg) => {
                Err(ApiError::BadParam {
                    msg: "invalid jscode".into(),
                    detail: format!("invalid jscode {jscode:?}: {msg}")
                        .into(),
                })
            }
            wechat::Error::RateLimited(msg) => {
                api_bail_status!("throttle", format!("throttle: {msg}"));
            }
            wechat::Error::HighRisk(msg) => {
                api_bail_status!(
                    "forbidden",
                    format!("code blocked: {msg}")
                );
            }
            err @ wechat::Error::System(_) => {
                api_bail!("{err}")
            }
            wechat::Error::Unknown(err) => Err(ApiError::Unknown(
                err.context("Wechat::jscode2session"),
            )),
            err => {
                api_bail!("Wechat::jscode2session Err unreachable: {err}")
            }
        },
    }
}
