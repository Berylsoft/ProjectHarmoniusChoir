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
    api_bail_status, api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PreSubmitReq {
    pid: i64,
    name: Box<str>,
    harmony_group_intention: Option<bool>,
    comment: Box<str>,

    file: PreSubmitFile,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PreSubmitFile {
    File(i64),
    Skip(Box<str>),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PreSubmitRes {
    Success,
    InvalidName,
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

#[expect(clippy::too_many_lines)]
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
        file,
    } = req;

    if !is_valid_name(&name) {
        tracing::debug!("invalid name");
        return Ok(PreSubmitRes::InvalidName);
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

        let stage = file::Stage::PreSubmit;
        let source = file::Source::new(pid, token.uid, None, stage);

        match &file {
            PreSubmitFile::File(file_id) => {
                let status =
                    file::Status::get_by_id(&mut trans, *file_id)
                        .await
                        .context("file::Status::get_by_id")?;
                if !matches!(status, Some(file::Status::Pending)) {
                    tracing::debug!(
                        "invalid file status for pre-submit: {status:?}"
                    );
                    api_bail_status!(
                        "invalid file",
                        format!(
                            "invalid file status \
for pre-submit: {status:?}"
                        )
                    );
                }

                let is_uploaded_by =
                    file::is_uploaded_by(&mut trans, *file_id, source)
                        .await?;
                if !is_uploaded_by {
                    api_bail_status!(
                        "invalid file",
                        format!(
                            "the user {} can't use this file",
                            token.uid
                        )
                    );
                }
            }
            PreSubmitFile::Skip(skip_pswd) => {
                // expect pid verified by verify_joined_project
                let p_skip_pswd: Box<str> =
                    sqlx::query_scalar(include_str!(
                        "./sqls/get_pre_submit_skip_password_by_pid.sql"
                    ))
                    .bind(pid)
                    .fetch_one(&mut *trans)
                    .await
                    .context("get_pre_submit_skip_password_by_pid")?;
                if skip_pswd != &p_skip_pswd {
                    return Ok(PreSubmitRes::InvalidSkipPassword);
                }
            }
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

        if let PreSubmitFile::File(file_id) = file {
            file::use_file(&mut trans, file_id, stage, pre_submit_id)
                .await?;
        }

        ApiResult::Ok(PreSubmitRes::Success)
    }
    .await;

    api::end_transaction(result, trans).await
}
