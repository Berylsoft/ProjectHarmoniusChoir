use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiError, ApiResult, ToJson, is_valid_name,
        shared::{file, project, project_user},
        user::UserToken,
    },
    api_bail, api_bail_status, api_begin_transaction,
    database::Database,
    extractors::Token,
    utils::length_check_quick,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PreSubmitReq {
    pid: i64,
    name: Box<str>,
    harmony_group_intention: Option<bool>,
    comment: Box<str>,

    skip: Option<Box<str>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PreSubmitRes {
    Success,
    InvalidName,
    InvalidComment,
    InvalidSkipPassword,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<PreSubmitReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let pre_submit_res =
        api::spawn_await(do_submit(db, token.0, req)).await??;

    Ok(Json(api::Response::Ok(pre_submit_res)))
}

#[expect(clippy::too_many_lines, clippy::cognitive_complexity)]
async fn do_submit(
    db: Database,
    token: UserToken,
    req: PreSubmitReq,
) -> ApiResult<PreSubmitRes, ToJson> {
    let PreSubmitReq {
        pid,
        name,
        harmony_group_intention: hgi,
        comment,
        skip,
    } = req;

    if !is_valid_name(&name) {
        tracing::debug!("invalid name");
        return Ok(PreSubmitRes::InvalidName);
    }

    if !length_check_quick(&comment, 200) {
        tracing::debug!("comment too long");
        return Ok(PreSubmitRes::InvalidComment);
    }

    if let Some(skip) = &skip
        && !length_check_quick(skip, 20)
    {
        tracing::debug!("skip password too long");
        return Ok(PreSubmitRes::InvalidSkipPassword);
    }

    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        let project_uid =
            token.verify_joined_project(&mut trans, pid, false).await?;

        // pid checked by verify_joined_project
        let info = project::Info::get_by_id(&mut trans, pid)
            .await
            .context("project::Info::get_by_id")?
            .context("pid verified by verify_joined_project")?;
        let require_hgi = info.require_harmony_group_intention;

        if hgi.is_some() != require_hgi {
            return Err(ApiError::BadParam {
                msg: "bad harmony_group_intention".into(),
                detail: format!(
                    "invalid HGI param: {hgi:?}, require: {require_hgi}"
                )
                .into(),
            });
        }

        let have_uploading =
            file::have_uploading(&mut trans, token.uid, None).await?;
        if have_uploading {
            api_bail_status!("have uploading files");
        }

        let stage = file::Stage::PreSubmit;
        let source = file::Source::new(pid, token.uid, None, stage);

        let pending = file::Info::get_pending(&mut trans, source).await?;

        if let Some(skip) = skip {
            if !pending.is_empty() {
                api_bail_status!(
                    "already uploaded file",
                    "can't use skip password when have file pending"
                );
            }

            // expect pid verified by verify_joined_project
            let skip_pswd: Box<str> = sqlx::query_scalar(include_str!(
                "./sqls/get_pre_submit_skip_password_by_pid.sql"
            ))
            .bind(pid)
            .fetch_one(&mut *trans)
            .await
            .context("get_pre_submit_skip_password_by_pid")?;
            if skip != skip_pswd {
                tracing::debug!("incorrect skip password");
                return Ok(PreSubmitRes::InvalidSkipPassword);
            }
        } else if pending.is_empty() {
            api_bail_status!("no available file");
        } else if pending.len() > 1 {
            api_bail!(
                "expect only allowed one file \
per project per user per pre-submit"
            );
        }

        let (status, _) =
            project_user::Status::get_by_puid(&mut trans, project_uid)
                .await
                .context("project_user::Status::get_by_puid")?;
        if !matches!(
            file::Stage::try_from(status),
            Ok(file::Stage::PreSubmit)
        ) {
            api_bail_status!(
                "invalid status",
                format!("invalid status for pre-submit: {status:?}")
            )
        }

        // ==================== write boundary ====================

        let pre_submit_id = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/ins_pre_submit.sql"
        ))
        .bind(project_uid)
        .bind(Utc::now().to_rfc3339())
        .bind(&*name)
        .bind(hgi)
        .bind(&*comment)
        .fetch_one(&mut *trans)
        .await
        .context("ins_pre_submit")?;

        if let Some(info) = pending.first() {
            file::use_file(&mut trans, info.id, stage, pre_submit_id)
                .await?;
        }

        ApiResult::Ok(PreSubmitRes::Success)
    }
    .await;

    api::end_transaction(result, trans).await
}
