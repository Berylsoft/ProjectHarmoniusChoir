use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToJson, shared::project_user, user::UserToken,
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
    status: project_user::Status,
    /// None for no nda or not applicable
    nda_agreed: Option<bool>,
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
        let puid =
            token.verify_joined_project(&mut trans, req.pid).await?;

        let (status, _) =
            project_user::Status::get_by_puid(&mut trans, puid)
                .await
                .context("project_user::Status::get_by_puid")?;

        if status < project_user::Status::PreSubmitPassed {
            return Ok(ProjectInfoRes {
                status,
                nda_agreed: None,
            });
        }

        // pid/puid is verified in token.verify_joined_project
        let nda_status = project_user::NdaStatus::get_by_pid_puid(
            &mut trans, req.pid, puid,
        )
        .await
        .context("project_user::NdaStatus::get_by_pid_puid")?;

        let nda_agreed = match nda_status {
            project_user::NdaStatus::NoNda => None,
            project_user::NdaStatus::Pending(_) => Some(false),
            project_user::NdaStatus::Agreed => Some(true),
        };

        ApiResult::Ok(ProjectInfoRes { status, nda_agreed })
    }
    .await;

    api::end_transaction(result, trans).await
}
