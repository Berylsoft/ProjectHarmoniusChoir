// NOTE: remove
#![expect(unused, reason = "template")]

use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{self, ApiResult, ToCbor, manager::ManagerToken, spawn_await},
    api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Req {}

#[derive(Debug, Serialize, Deserialize)]
pub struct Res {}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<Req>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    // NOTE: api_param_assert
    let response = spawn_await(do_(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_(
    db: Database,
    token: ManagerToken,
    req: Req,
) -> ApiResult<Res, ToCbor> {
    // NOTE: decide the begin mode
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        // NOTE: decide the verify permission
        token.verify_sudo(&mut trans, true).await?;

        ApiResult::Ok(Res {})
    }
    .await;

    api::end_transaction(result, trans).await
}
