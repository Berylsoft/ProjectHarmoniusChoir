use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
    ServerState,
    api::{self, ApiResult, ToCbor, manager::ManagerToken, spawn_await},
    api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListManagersRes {
    managers: Vec<ManagerInfo>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ManagerInfo {
    id: i64,
    name: Box<str>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<()>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    req.0.verified(&mut cache).await?;

    let response = spawn_await(do_list_managers(db, token.0)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_list_managers(
    db: Database,
    token: ManagerToken,
) -> ApiResult<ListManagersRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify_root(&mut trans).await?;

        let managers: Vec<ManagerInfo> =
            sqlx::query_as(include_str!("./sqls/get_managers.sql"))
                .fetch_all(&mut *trans)
                .await
                .context("get_managers")?;

        ApiResult::Ok(ListManagersRes { managers })
    }
    .await;

    api::end_transaction(result, trans).await
}
