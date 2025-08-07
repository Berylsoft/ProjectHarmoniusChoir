use axum::{extract::State, response::IntoResponse};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{self, ApiResult, ToCbor, manager::ManagerToken, spawn_await},
    api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
    utils::cookie_set_token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct AcquireSudoReq {
    totp_code: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AcquireSudoRes {
    Success,
    InvalidCredential,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<AcquireSudoReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState {
        mut cache,
        db,
        ref key,
        ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let token = spawn_await(do_acquire_sudo(db, token.0, req)).await??;

    match token {
        Ok(token) => Ok((
            [cookie_set_token(token, key)],
            Cbor(api::Response::Ok(AcquireSudoRes::Success)),
        )
            .into_response()),
        Err(err) => Ok(Cbor(api::Response::Ok(err)).into_response()),
    }
}

async fn do_acquire_sudo(
    db: Database,
    token: ManagerToken,
    req: AcquireSudoReq,
) -> ApiResult<Result<ManagerToken, AcquireSudoRes>, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;
        let valid = token.verify_totp(&mut trans, req.totp_code).await?;

        if !valid {
            tracing::debug!("invalid totp code");
            return Ok(Err(AcquireSudoRes::InvalidCredential));
        }

        ApiResult::<_, _>::Ok(Ok(ManagerToken {
            sudo_expired: Utc::now() + TimeDelta::minutes(10),
            ..token
        }))
    }
    .await;

    api::end_transaction(result, trans).await
}
