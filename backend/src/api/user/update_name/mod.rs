use anyhow::Context;
use axum::{
    extract::{Json, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use super::UserToken;
use crate::{
    ServerState,
    api::{self, ApiResult, Response, ToJson, end_transaction},
    api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateName {
    new_name: String,
}

pub(crate) async fn router(
    mut state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<UpdateName>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let req = req.0.verified(&mut state.0.cache).await?;

    tokio::task::spawn(do_update_name(state.0.db, token.0, req.new_name))
        .await
        .context("join tokio task")??;

    Ok(Json(Response::Ok(())))
}

async fn do_update_name(
    db: Database,
    token: UserToken,
    new_name: String,
) -> ApiResult<(), ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let res = async {
        token.verify(&mut trans).await?;
        let uid = token.uid;

        let previous_name = sqlx::query_scalar::<_, String>(
            include_str!("./sqls/get_name.sql"),
        )
        .bind(uid)
        .fetch_one(&mut *trans)
        .await
        .context("get_name")?;

        if previous_name != new_name {
            sqlx::query(include_str!("./sqls/update_name.sql"))
                .bind(new_name)
                .bind(uid)
                .execute(&mut *trans)
                .await
                .context("update_name")?;
        }

        ApiResult::<_, _>::Ok(())
    }
    .await;

    end_transaction(res, trans).await
}
