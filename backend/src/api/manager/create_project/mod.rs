use anyhow::Context;
use axum::{
    body::Body, extract::State, http::Response, response::IntoResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    S3, ServerState,
    api::{
        self, ApiResult, ToCbor, end_transaction, manager::ManagerToken,
        shared::file, spawn_await,
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
    pre_submit_file_size_min: i64,
    pre_submit_file_size_max: i64,
    submit_file_size_min: i64,
    submit_file_size_max: i64,
    master_file_size_max: i64,
    end_time: DateTime<Utc>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<CreateProjectReq>>,
) -> api::ApiResult<Response<Body>, ToCbor> {
    let ServerState {
        mut cache, db, s3, ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    spawn_await(do_create_project(s3, db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(())).into_response())
}

async fn do_create_project(
    s3: S3,
    db: Database,
    token: ManagerToken,
    req: CreateProjectReq,
) -> ApiResult<(), ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result: ApiResult<(), ToCbor> = async {
        token.verify_sudo(&mut trans, true).await?;

        if let Some(key) = req.attachment_key.as_deref() {
            let exists = file::is_s3_file_exists(&s3, key)
                .await
                .context("file::is_s3_file_exists")?;
            if !exists {
                return Err(api::ApiError::BadParam {
                    msg: "attachment not found".into(),
                    detail: format!("attachment {key} not found in s3")
                        .into_boxed_str(),
                });
            }
        }

        let create_result =
            sqlx::query(include_str!("./sqls/create_project.sql"))
                .bind(req.name)
                .bind(req.entry_question)
                .bind(req.entry_answer)
                .bind(req.pre_submit_skip_password)
                .bind(req.require_harmony_group_intention)
                .bind(req.non_disclosure_agreement)
                .bind(req.attachment_key)
                .bind(req.pre_submit_file_size_min)
                .bind(req.pre_submit_file_size_max)
                .bind(req.submit_file_size_min)
                .bind(req.submit_file_size_max)
                .bind(req.master_file_size_max)
                .bind(req.end_time.to_rfc3339())
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
