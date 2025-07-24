use axum::{extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor, manager::ManagerToken, shared::file,
        spawn_await,
    },
    api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteFileReq {
    file_id: i64,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<DeleteFileReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    spawn_await(do_delete_file(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(())))
}

async fn do_delete_file(
    db: Database,
    token: ManagerToken,
    req: DeleteFileReq,
) -> ApiResult<(), ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;

        file::delete(&mut trans, req.file_id, 0, Some(token.mid)).await?;

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}
