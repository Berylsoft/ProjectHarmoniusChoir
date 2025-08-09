use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToJson,
        shared::{file, project_user, submit},
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
            return Ok(ListPendingFilesRes { files: vec![] });
        };

        let source = file::Source::new(req.pid, token.uid, None, stage);

        let mut files =
            file::Info::get_pending(&mut trans, source).await?;

        if stage == file::Stage::Submit {
            let group_info =
                submit::GroupInfo::get_by_puid(&mut trans, puid).await?;
            if group_info.choir
                && let Some(file_info) =
                    file::Info::get_of_latest_pre_submit_by_puid(
                        &mut trans, puid,
                    )
                    .await?
            {
                let file_status =
                    file::Status::get_by_id(&mut trans, file_info.id)
                        .await
                        .context("file::Status::get_by_id")?
                        .context("expect file exists")?;

                // the file is got by the latest pre submit
                // so expect it's `file::Status::Used`
                // so expect the `uses` must include the pre submit
                // so expect when `len` == 1, it only include the pre submit
                if let file::Status::Used(uses) = file_status
                    && uses.len() == 1
                {
                    files.push(file_info);
                }
            }
        }

        ApiResult::Ok(ListPendingFilesRes { files })
    }
    .await;

    api::end_transaction(result, trans).await
}
