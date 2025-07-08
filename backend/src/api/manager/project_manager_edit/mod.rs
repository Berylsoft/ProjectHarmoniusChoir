use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{self, ApiResult, ToCbor, manager::ManagerToken, spawn_await},
    api_assert, api_begin_transaction, api_param_assert,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectManagerEditReq {
    pid: i64,
    mid: i64,
    is_revoke: bool,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<ProjectManagerEditReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    api_param_assert!(req.pid >= 1);
    api_param_assert!(req.mid >= 1);
    spawn_await(do_project_manager_edit(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(())))
}

async fn do_project_manager_edit(
    db: Database,
    token: ManagerToken,
    req: ProjectManagerEditReq,
) -> ApiResult<(), ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify_sudo(&mut trans, true).await?;

        let ins_project_manager_result =
            sqlx::query(include_str!("./sqls/ins_project_manager.sql"))
                .bind(req.pid)
                .bind(req.mid)
                .bind(req.is_revoke)
                .execute(&mut *trans)
                .await
                .context("ins_project_manager_result")?;

        api_assert!(ins_project_manager_result.rows_affected() == 1);

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}
