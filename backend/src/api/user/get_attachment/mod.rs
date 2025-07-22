use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    S3, ServerState,
    api::{
        self, ApiResult, ToJson,
        shared::{file, project, project_user},
        user::UserToken,
    },
    api_bail_not_found, api_bail_status, api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct GetAttachmentReq {
    pid: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetAttachmentRes {
    presigned_req: file::PresignedReq,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<GetAttachmentReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState {
        mut cache, db, s3, ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let response =
        api::spawn_await(do_get_attachment(db, s3, token.0, req))
            .await??;

    Ok(Json(api::Response::Ok(response)))
}

async fn do_get_attachment(
    db: Database,
    s3: S3,
    token: UserToken,
    req: GetAttachmentReq,
) -> ApiResult<GetAttachmentRes, ToJson> {
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
            api_bail_status!(
                "invalid status",
                format!("before pre_submit: {status:?}")
            );
        }

        let nda_status = project_user::NdaStatus::get_by_pid_puid(
            &mut trans, req.pid, puid,
        )
        .await
        .context("project_user::NdaStatus::get_by_pid_puid")?;

        if !nda_status.is_agreed_or_no_nda() {
            api_bail_status!("invalid status", "nda pending");
        }

        // pid verified by verify_joined_project
        let key = project::get_attachment_key_by_id(&mut trans, req.pid)
            .await?;
        let Some(key) = key else {
            api_bail_not_found!("no attachment");
        };

        let presigned_req = file::pre_signed_get_simple(&s3, key).await?;

        ApiResult::Ok(GetAttachmentRes { presigned_req })
    }
    .await;

    api::end_transaction(result, trans).await
}
