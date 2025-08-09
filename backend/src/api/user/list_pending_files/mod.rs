use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToJson,
        shared::{
            file::{self, get_pre_submit_file_for_submit},
            project_user,
        },
        user::UserToken,
    },
    api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListPendingFilesReq {
    pid: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListPendingFilesRes {
    files: Vec<file::Info>,
    pre_submit_file: Option<file::Info>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<ListPendingFilesReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let response =
        api::spawn_await(do_list_pending_files(db, token.0, req))
            .await??;

    Ok(Json(api::Response::Ok(response)))
}

async fn do_list_pending_files(
    db: Database,
    token: UserToken,
    req: ListPendingFilesReq,
) -> ApiResult<ListPendingFilesRes, ToJson> {
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

        let Ok(stage) = file::Stage::try_from(status) else {
            return Ok(ListPendingFilesRes {
                files: vec![],
                pre_submit_file: None,
            });
        };

        let source = file::Source::new(req.pid, token.uid, None, stage);

        let files = file::Info::get_pending(&mut trans, source).await?;
        let pre_submit_file =
            get_pre_submit_file_for_submit(&mut trans, puid)
                .await
                .context("file::get_pre_submit_file_for_submit")?;

        ApiResult::Ok(ListPendingFilesRes {
            files,
            pre_submit_file,
        })
    }
    .await;

    api::end_transaction(result, trans).await
}
