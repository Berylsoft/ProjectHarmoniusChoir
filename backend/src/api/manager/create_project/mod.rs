use anyhow::Context;
use axum::{
    body::Body, extract::State, http::Response, response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor, end_transaction, manager::ManagerToken,
        spawn_await,
    },
    api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateProjectReq {
    name: String,
    entry_question: String,
    entry_answer: String,
    pre_submit_skip_password: String,
    require_harmony_group_intention: bool,
    non_disclosure_agreement: Option<String>,
    attachment_key: Option<String>,
    pre_submit_file_size_min: u64,
    pre_submit_file_size_max: u64,
    submit_file_size_min: u64,
    submit_file_size_max: u64,
    master_file_size_max: u64,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<CreateProjectReq>>,
) -> api::ApiResult<Response<Body>, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    spawn_await(do_create_project(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(())).into_response())
}

async fn do_create_project(
    db: Database,
    token: ManagerToken,
    req: CreateProjectReq,
) -> ApiResult<(), ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result: ApiResult<(), ToCbor> = async {
        token.verify_sudo(&mut trans, true).await?;

        let create_result =
            sqlx::query(include_str!("./sqls/create_project.sql"))
                .bind(req.name)
                .execute(&mut *trans)
                .await
                .context("create_project")?;

        if create_result.rows_affected() != 1 {
            return Err(api::ApiError::Unknown(anyhow::anyhow!(
                "create_project => rows_affected != 1"
            )));
        }

        Ok(())
    }
    .await;

    end_transaction(result, trans).await
}
