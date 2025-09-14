use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState, UnameUid,
    api::{
        self, ApiResult, ToJson, get_openid_by_jscode, user::UserToken,
    },
    api_assert, api_bail_not_found, api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct WechatClaimReq {
    code: Box<str>,
    uname: Box<str>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WechatClaimRes {
    InvalidReq,
    Success,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<WechatClaimReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState {
        mut cache,
        db,
        wechat,
        uname_uid,
        ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let wechat_openid =
        get_openid_by_jscode::<ToJson>(&*wechat, &req.code)
            .await
            .context("get_openid_by_jscode")?;

    let response = api::spawn_await(do_wechat_claim(
        db,
        uname_uid,
        token.0,
        req,
        wechat_openid,
    ))
    .await??;

    Ok(Json(api::Response::Ok(response)))
}

async fn do_wechat_claim(
    db: Database,
    uname_uid: UnameUid,
    token: UserToken,
    req: WechatClaimReq,
    wechat_openid: Box<str>,
) -> ApiResult<WechatClaimRes, ToJson> {
    let Some(uid) = uname_uid.get(&req.uname).copied() else {
        api_bail_not_found!("invalid uname");
    };

    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;

        let is_bound: i64 =
            sqlx::query_scalar(include_str!("./sqls/is_uid_bound.sql"))
                .bind(uid)
                .fetch_one(&mut *trans)
                .await
                .context("is_uid_bound")?;

        if is_bound > 1 {
            return Ok(WechatClaimRes::InvalidReq);
        }

        let is_bound: i64 = sqlx::query_scalar(include_str!(
            "./sqls/is_openid_bound.sql"
        ))
        .bind(&wechat_openid)
        .fetch_one(&mut *trans)
        .await
        .context("is_openid_bound")?;

        if is_bound > 1 {
            return Ok(WechatClaimRes::InvalidReq);
        }

        // ==================== write boundary ====================

        let set_result =
            sqlx::query(include_str!("./sqls/set_openid_by_uid.sql"))
                .bind(wechat_openid)
                .bind(uid)
                .execute(&mut *trans)
                .await
                .context("set_openid_by_uid")?;

        api_assert!(set_result.rows_affected() == 1);

        ApiResult::Ok(WechatClaimRes::Success)
    }
    .await;

    api::end_transaction(result, trans).await
}
