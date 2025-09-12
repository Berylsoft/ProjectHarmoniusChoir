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
    api_bail_status, api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitReq {
    pid: i64,
    comment: Box<str>,
    include_pre_submit_file: bool,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<SubmitReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    api::spawn_await(do_submit(db, token.0, req)).await??;

    Ok(Json(api::Response::Ok(())))
}

async fn do_submit(
    db: Database,
    token: UserToken,
    req: SubmitReq,
) -> ApiResult<(), ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        let puid = token
            .verify_joined_project(&mut trans, req.pid, false)
            .await?;

        let nda_status = project_user::NdaStatus::get_by_pid_puid(
            &mut trans, req.pid, puid,
        )
        .await
        .context("project_user::NdaStatus::get_by_pid_puid")?;

        if matches!(nda_status, project_user::NdaStatus::Pending(_)) {
            api_bail_status!(
                "invalid status",
                format!("nda_status: {nda_status:?}")
            );
        }

        let (status, _) =
            project_user::Status::get_by_puid(&mut trans, puid)
                .await
                .context("project_user::Status::get_by_puid")?;

        let stage = file::Stage::try_from(status);
        let stage = if let Ok(stage) = stage
            && stage == file::Stage::Submit
        {
            stage
        } else {
            api_bail_status!(
                "invalid status",
                format!("status: {status:?}")
            );
        };

        let have_uploading =
            file::have_uploading(&mut trans, token.uid, None).await?;
        if have_uploading {
            api_bail_status!("have uploading files")
        }

        let source = file::Source::new(req.pid, token.uid, None, stage);

        let mut pending =
            file::Info::get_pending(&mut trans, source).await?;
        let from_pre_submit =
            file::get_pre_submit_file_for_submit(&mut trans, puid)
                .await
                .context("file::get_pre_submit_file_for_submit")?;
        if req.include_pre_submit_file
            && let Some(pre_submit) = from_pre_submit
        {
            pending.push(pre_submit);
        }

        if pending.is_empty() {
            api_bail_status!("no available file");
        }

        // ==================== write boundary ====================
        let sid: i64 =
            sqlx::query_scalar(include_str!("./sqls/ins_submit.sql"))
                .bind(puid)
                .bind(Utc::now().to_rfc3339())
                .bind(&*req.comment)
                .fetch_one(&mut *trans)
                .await
                .context("ins_submit")?;

        for info in pending {
            file::use_file(&mut trans, info.id, stage, sid).await?;
        }

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}
