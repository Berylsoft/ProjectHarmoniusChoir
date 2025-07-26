use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{self, ApiResult, ToJson, user::UserToken},
    api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct GetInfoRes {
    id: i64,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<()>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    req.0.verified(&mut cache).await?;

    let response = api::spawn_await(do_get_info(db, token.0)).await??;

    Ok(Json(api::Response::Ok(response)))
}

async fn do_get_info(
    db: Database,
    token: UserToken,
) -> ApiResult<GetInfoRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;

        ApiResult::Ok(GetInfoRes { id: token.uid })
    }
    .await;

    api::end_transaction(result, trans).await
}
