use axum::{extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{self, ApiResult, ToCbor, manager::ManagerToken, spawn_await},
    api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct GetInfoRes {
    id: i64,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<()>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    req.0.verified(&mut cache).await?;

    let response = spawn_await(do_get_info(db, token.0)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_get_info(
    db: Database,
    token: ManagerToken,
) -> ApiResult<GetInfoRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;

        ApiResult::Ok(GetInfoRes { id: token.mid })
    }
    .await;

    api::end_transaction(result, trans).await
}
