use axum::{extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::ManagerToken,
        shared::{file, project_user},
        spawn_await,
    },
    api_bail_not_found, api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListPendingFilesReq {
    puid: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListPendingFilesRes {
    files: Vec<file::Info>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<ListPendingFilesReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let response =
        spawn_await(do_pending_files(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_pending_files(
    db: Database,
    token: ManagerToken,
    req: ListPendingFilesReq,
) -> ApiResult<ListPendingFilesRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;

        let pid_uid =
            project_user::get_pid_uid_by_id(&mut trans, req.puid).await?;

        let Some((pid, uid)) = pid_uid else {
            api_bail_not_found!(
                "project user not found",
                format!("can't find project user by puid: {}", req.puid)
            )
        };

        token.verify_can_access_project(&mut trans, pid).await?;

        let source = file::Source::new(
            pid,
            uid,
            Some(token.mid),
            file::Stage::PreSubmit,
        );

        let files = file::Info::get_pending(&mut trans, source).await?;

        ApiResult::Ok(ListPendingFilesRes { files })
    }
    .await;

    api::end_transaction(result, trans).await
}
