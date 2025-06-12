use std::{borrow::Cow, marker::PhantomData};

use anyhow::Context;
use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::{
    database::try_end_transaction, extractors::Cbor, impl_deref,
};

pub mod user;

#[derive(Debug, Serialize, Deserialize)]
pub struct Request<T> {
    data: T,
    // TODO:
    nonce: Option<Ulid>,
}

impl_deref!(impl<T> ref Request<T> => T = .data);

#[derive(Debug, Serialize, Deserialize)]
pub enum Response<'msg, T> {
    Ok(T),
    Err { code: ErrCode, msg: Cow<'msg, str> },
}

#[derive(Debug, Serialize, Deserialize)]
#[repr(u64)]
pub enum ErrCode {
    Unknown = 1,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError<T> {
    #[error("{0}")]
    Unknown(#[from] anyhow::Error),
    #[error("response serialization type marker")]
    __(PhantomData<T>),
}

impl<T> ApiError<T> {
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
            Self::__(_) => unreachable!(),
        }
    }
}

pub type ApiResult<T, S> = Result<T, ApiError<S>>;

pub struct ToJson();
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
macro_rules! begin_transaction {
    ($db:expr, $conn:ident, $trans:ident, $stmt:ident) => {
        let mut $conn = ::anyhow::Context::context(
            $db.acquire().await,
            "db connection acquire",
        )?;

        let mut $trans = ::anyhow::Context::context(
            $conn.begin_with($crate::database::BeginStmt::$stmt).await,
            "transaction begin",
        )?;
    };
}

pub async fn end_transaction<DB, R>(
    result: anyhow::Result<R>,
    trans: sqlx::Transaction<'_, DB>,
) -> anyhow::Result<R>
where
    DB: sqlx::Database,
{
    try_end_transaction(result, trans)
        .await
        .context("transaction end")
}
