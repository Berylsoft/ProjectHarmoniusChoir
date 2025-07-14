use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToJson,
        shared::{file, project_user},
        user::UserToken,
    },
    api_assert, api_begin_transaction,
    database::{Database, last_insert_rowid},
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum PreSubmitReq {
    Info {
        pid: i64,
    },
    Submit {
        pid: i64,
        harmony_group_intention: Option<bool>,
        comment: Box<str>,

        file_id: i64,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PreSubmitRes {
    Info {
        require_harmony_group_intention: bool,
    },
    Success,
    InvalidStatus,
    InvalidHgi,
    InvalidFile,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<PreSubmitReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    // NOTE: api_param_assert
    let pre_submit_res = match req {
        PreSubmitReq::Info { pid } => {
            api::spawn_await(do_info(db, token.0, pid)).await??
        }
        PreSubmitReq::Submit {
            pid,
            harmony_group_intention,
            comment,
            file_id,
        } => {
            api::spawn_await(do_submit(
                db,
                token.0,
                pid,
                harmony_group_intention,
                comment,
                file_id,
            ))
            .await??
        }
    };

    Ok(Json(api::Response::Ok(pre_submit_res)))
}

async fn do_info(
    db: Database,
    token: UserToken,
    pid: i64,
) -> ApiResult<PreSubmitRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;
        let _ = token.verify_joined_project(&mut trans, pid).await?;

        // TODO: maybe not allow calling this when the user can't do pre-submit

        // pid checked by verify_joined_project
        let require_harmony_group_intention =
            is_project_require_harmony_group_intention_by_pid(
                &mut trans, pid,
            )
            .await?;

        ApiResult::Ok(PreSubmitRes::Info {
            require_harmony_group_intention,
        })
    }
    .await;

    api::end_transaction(result, trans).await
}

async fn do_submit(
    db: Database,
    token: UserToken,
    pid: i64,
    hgi: Option<bool>,
    comment: Box<str>,
    file_id: i64,
) -> ApiResult<PreSubmitRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        let project_uid =
            token.verify_joined_project(&mut trans, pid).await?;

        // pid checked by verify_joined_project
        let require_hgi =
            is_project_require_harmony_group_intention_by_pid(
                &mut trans, pid,
            )
            .await?;
        if hgi.is_some() != require_hgi {
            tracing::debug!(
                "invalid HGI param: {hgi:?}, require: {require_hgi}"
            );
            return Ok(PreSubmitRes::InvalidHgi);
        }

        let stage = file::Stage::PreSubmit;
        let source = file::Source::new(pid, token.uid, None, stage);

        let status = file::Status::get_by_id(&mut trans, file_id)
            .await
            .context("file::Status::get_by_id")?;
        if !matches!(status, Some(file::Status::Pending)) {
            tracing::debug!(
                "invalid file status for pre-submit: {status:?}"
            );
            return Ok(PreSubmitRes::InvalidFile);
        }

        let can_use = file::can_use(&mut trans, file_id, source).await?;
        if !can_use {
            tracing::debug!("the user can't use this file");
            return Ok(PreSubmitRes::InvalidFile);
        }

        let (status, _) =
            project_user::Status::get_by_puid(&mut trans, project_uid)
                .await
                .context("project_user::Status::get_by_puid")?;
        if !matches!(
            file::Stage::try_from(status),
            Ok(file::Stage::PreSubmit)
        ) {
            tracing::debug!("invalid status for pre-submit: {status:?}");
            return Ok(PreSubmitRes::InvalidStatus);
        }

        let ins_result =
            sqlx::query(include_str!("./sqls/ins_pre_submit.sql"))
                .bind(project_uid)
                .bind(Utc::now().to_rfc3339())
                .bind(hgi)
                .bind(&*comment)
                .execute(&mut *trans)
                .await
                .context("ins_pre_submit")?;

        api_assert!(ins_result.rows_affected() == 1);

        let pre_submit_id = last_insert_rowid(&mut trans).await?;

        file::use_file(&mut trans, file_id, stage, pre_submit_id).await?;

        ApiResult::Ok(PreSubmitRes::Success)
    }
    .await;

    api::end_transaction(result, trans).await
}

/// expect `pid` valid
async fn is_project_require_harmony_group_intention_by_pid(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    pid: i64,
) -> anyhow::Result<bool> {
    sqlx::query_scalar(include_str!(
        "./sqls/get_require_harmony_group_intention_by_pid.sql"
    ))
    .bind(pid)
    .fetch_one(&mut **trans)
    .await
    .context("get_require_harmony_group_intention_by_pid")
}
