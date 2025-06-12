use anyhow::Context;
use axum::{
    extract::{Json, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use super::UserToken;
use crate::{
    ServerState,
    api::{self, ApiError, ApiResult, Response, ToJson, end_transaction},
    begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateName {
    new_name: String,
}

pub async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<UpdateName>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    token.verify(&state.db).await?;

    tokio::task::spawn(do_update_name(
        state.0.db,
        token.uid,
        req.0.data.new_name,
    ))
    .await
    .context("join tokio task")??;

    Ok(Json(Response::Ok(())))
}

pub async fn do_update_name(
    db: Database,
    uid: i64,
    new_name: String,
) -> ApiResult<(), ToJson> {
    begin_transaction!(db, conn, trans, Immediate);

    let res = async {
        let previous_name = sqlx::query_scalar::<_, String>(
            include_str!("./sqls/get_name.sql"),
        )
        .bind(uid)
        .fetch_optional(&mut *trans)
        .await
        .context("get_name")?;

        let previous_name =
            previous_name.ok_or(ApiError::<ToJson>::UserNotExists)?;

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
