// NOTE: remove
#![expect(unused, reason = "template")]

use anyhow::Context as _;
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
pub struct Req {}

#[derive(Debug, Serialize, Deserialize)]
pub struct Res {}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<Req>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    // NOTE: api_param_assert
    let response = api::spawn_await(do_(db, token.0, req)).await??;

    Ok(Json(api::Response::Ok(response)))
}

async fn do_(
    db: Database,
    token: UserToken,
    req: Req,
) -> ApiResult<Res, ToJson> {
    // NOTE: decide the begin mode
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;
        // NOTE: pid
        #[expect(unused, reason = "template")]
        let puid =
            token.verify_joined_project(&mut trans, 0, true).await?;

        std::hint::black_box(req);

        ApiResult::Ok(Res {})
    }
    .await;

    api::end_transaction(result, trans).await
}
