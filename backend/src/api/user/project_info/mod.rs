use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToJson,
        shared::{project, project_user, submit},
        user::UserToken,
    },
    api_begin_transaction, api_param_assert,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectInfoReq {
    pid: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectInfoRes {
    info: project::Info,
    status: project_user::Status,
    /// if passed/rejected
    pre_submit_detail: Option<submit::PreSubmitStatus>,
    /// None for not applicable
    nda_info: Option<project_user::NdaStatus>,
    /// after pre-submit passed and nda agreed
    have_attachment: Option<bool>,
    /// if rejected
    submit_detail: Option<submit::SubmitStatus>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<ProjectInfoReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    api_param_assert!(req.pid >= 1);
    let info =
        api::spawn_await(do_project_info(db, token.0, req)).await??;

    Ok(Json(api::Response::Ok(info)))
}

async fn do_project_info(
    db: Database,
    token: UserToken,
    req: ProjectInfoReq,
) -> ApiResult<ProjectInfoRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;
        let puid = token
            .verify_joined_project(&mut trans, req.pid, true)
            .await?;

        let (status, _) =
            project_user::Status::get_by_puid(&mut trans, puid)
                .await
                .context("project_user::Status::get_by_puid")?;

        let info = project::Info::get_by_id(&mut trans, req.pid)
            .await
            .context("project::Info::get_by_id")?
            .context("pid verified by verify_joined_project")?;

        let status = if status > project_user::Status::SubmitPassed {
            project_user::Status::SubmitPassed
        } else {
            status
        };

        let pre_submit_detail = if status
            > project_user::Status::PreSubmitted
        {
            Some(
                submit::PreSubmitStatus::get_by_puid(&mut trans, puid)
                    .await?,
            )
        } else {
            None
        };

        if status < project_user::Status::PreSubmitPassed {
            return Ok(ProjectInfoRes {
                info,
                status,
                pre_submit_detail,
                nda_info: None,
                have_attachment: None,
                submit_detail: None,
            });
        }

        let submit_detail =
            if status == project_user::Status::SubmitRejected {
                Some(
                    submit::SubmitStatus::get_by_puid(&mut trans, puid)
                        .await?,
                )
            } else {
                None
            };

        // pid/puid is verified in token.verify_joined_project
        let nda_status = project_user::NdaStatus::get_by_pid_puid(
            &mut trans, req.pid, puid,
        )
        .await
        .context("project_user::NdaStatus::get_by_pid_puid")?;

        let have_attachment = if nda_status.is_agreed_or_no_nda() {
            // pid verified by verify_joined_project
            Some(
                project::get_attachment_key_by_id(&mut trans, req.pid)
                    .await?
                    .is_some(),
            )
        } else {
            None
        };

        ApiResult::Ok(ProjectInfoRes {
            info,
            status,
            pre_submit_detail,
            nda_info: Some(nda_status),
            have_attachment,
            submit_detail,
        })
    }
    .await;

    api::end_transaction(result, trans).await
}
