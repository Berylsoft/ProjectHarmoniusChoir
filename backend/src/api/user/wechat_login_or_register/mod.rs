use anyhow::{Context, ensure};
use axum::{
    extract::{Json, State},
    response::IntoResponse,
};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Connection;

use super::UserToken;
use crate::{
    ServerState,
    api::{self, ToJson},
    begin_transaction,
    database::try_end_transaction,
    utils::cookie_set_token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct WechatLoginOrRegisterReq {
    code: String,
}

pub async fn router(
    state: State<ServerState>,
    req: Json<api::Request<WechatLoginOrRegisterReq>>,
) -> api::ApiResult<impl IntoResponse, ToJson> {
    // TODO: actual wechat auth
    tracing::info!("{req:?}");
    let wechat_openid = &req.code as &str;

    begin_transaction!(state.db, conn, trans);

    let mut run_migrations = async || {
        let exists_user = sqlx::query_as::<_, (i64, i64)>(include_str!(
            "./sqls/get_user_by_wechat_openid.sql"
        ))
        .bind(wechat_openid)
        .fetch_optional(&mut *trans)
        .await
        .context("fetch exists user by wechat openid")?;

        let user = if let Some(it) = exists_user {
            it
        } else {
            let cur_uid = sqlx::query_scalar::<_, i64>(include_str!(
                "./sqls/get_latest_uid.sql"
            ))
            .fetch_one(&mut *trans)
            .await
            .context("fetch latest uid")?;

            let uid = cur_uid + 1;
            let token_id = 1;

            let ins_result =
                sqlx::query(include_str!("./sqls/insert_new_user.sql"))
                    .bind(uid)
                    .bind(1)
                    .bind(false)
                    .bind(format!("U_{uid}"))
                    .bind(token_id)
                    .bind(wechat_openid)
                    .execute(&mut *trans)
                    .await
                    .context("insert new user")?;
            ensure!(ins_result.rows_affected() == 1);

            (uid, token_id)
        };

        anyhow::Result::<(_, _)>::Ok(user)
    };

    let (uid, token_id) =
        try_end_transaction(run_migrations().await, trans)
            .await
            .context("transaction end")?;

    Ok(([cookie_set_token(
        UserToken {
            uid,
            token_id,
            expired: Utc::now() + TimeDelta::days(7),
        },
        &state.key,
    )],))
}
