use anyhow::{Context, ensure};
use axum::{
    extract::{Json, State},
    response::IntoResponse,
};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use super::UserToken;
use crate::{
    ServerState,
    api::{
        self, ApiResult, Response, ToJson, end_transaction, spawn_await,
    },
    api_begin_transaction,
    database::Database,
    utils::cookie_set_token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct WechatLoginOrRegisterReq {
    code: String,
}

pub(crate) async fn router(
    mut state: State<ServerState>,
    req: Json<api::Request<WechatLoginOrRegisterReq>>,
) -> api::ApiResult<impl IntoResponse, ToJson> {
    let req = req.0.verified(&mut state.0.cache).await?;

    // TODO: actual wechat auth
    tracing::info!("{req:?}");
    let wechat_openid = req.code;

    let (uid, token_id) =
        spawn_await(do_register_or_login(state.0.db, wechat_openid))
            .await??;

    Ok((
        [cookie_set_token(
            UserToken {
                uid,
                token_id,
                expired: Utc::now() + TimeDelta::days(7),
            },
            &state.0.key,
        )],
        Json(Response::Ok(())),
    ))
}

async fn do_register_or_login(
    db: Database,
    wechat_openid: String,
) -> ApiResult<(i64, i64), ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let res = async {
        let exists_user = sqlx::query_as::<_, (i64, i64)>(include_str!(
            "./sqls/get_user_by_wechat_openid.sql"
        ))
        .bind(&wechat_openid)
        .fetch_optional(&mut *trans)
        .await
        .context("get exists user by wechat openid")?;

        let user = if let Some(it) = exists_user {
            it
        } else {
            let cur_uid = sqlx::query_scalar::<_, i64>(include_str!(
                "./sqls/get_latest_uid.sql"
            ))
            .fetch_one(&mut *trans)
            .await
            .context("get latest uid")?;

            let uid = cur_uid + 1;

            let ins_result =
                sqlx::query(include_str!("./sqls/ins_new_user.sql"))
                    .bind(uid)
                    .bind(format!("U_{uid}"))
                    .bind(wechat_openid)
                    .execute(&mut *trans)
                    .await
                    .context("ins new user")?;
            ensure!(ins_result.rows_affected() == 1);

            (uid, 1)
        };

        Ok(user)
    }
    .await;

    end_transaction(res, trans).await
}
