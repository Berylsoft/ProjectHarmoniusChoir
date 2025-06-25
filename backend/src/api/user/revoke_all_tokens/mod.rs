use anyhow::Context;
use axum::{
    extract::{Json, State},
    response::IntoResponse,
};

use super::UserToken;
use crate::{
    ServerState,
    api::{self, ApiError, ApiResult, Response, ToJson, end_transaction},
    api_begin_transaction,
    database::Database,
    extractors::Token,
};

pub(crate) async fn router(
    mut state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<()>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    req.0.verified(&mut state.cache).await?;

    tokio::task::spawn(do_revoke(state.0.db, token.0))
        .await
        .context("join tokio task")??;

    Ok(Json(Response::Ok(())))
}

async fn do_revoke(
    db: Database,
    token: UserToken,
) -> ApiResult<(), ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let res = async {
        token.verify(&mut trans).await?;
        let uid = token.uid;

        let res = sqlx::query(include_str!("./sqls/inc_token_id.sql"))
            .bind(uid)
            .execute(&mut *trans)
            .await
            .context("inc_token_id")?;

        if res.rows_affected() != 1 {
            return Err(ApiError::Unknown(anyhow::anyhow!(
                "assertion failed: res.rows_affected() == 1"
            )));
        }

        Ok(())
    }
    .await;

    end_transaction(res, trans).await
}
