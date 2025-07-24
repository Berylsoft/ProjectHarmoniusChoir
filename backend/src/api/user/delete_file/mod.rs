use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{self, ApiResult, ToJson, shared::file, user::UserToken},
    api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteFileReq {
    file_id: i64,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<DeleteFileReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    api::spawn_await(do_delete_file(db, token.0, req)).await??;

    Ok(Json(api::Response::Ok(())))
}

async fn do_delete_file(
    db: Database,
    token: UserToken,
    req: DeleteFileReq,
) -> ApiResult<(), ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        file::verify_project_not_ended_by_id(&mut trans, req.file_id)
            .await?;

        file::delete(&mut trans, req.file_id, token.uid, None).await?;

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}
