use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use ed25519_dalek::SigningKey;
use redis::aio::MultiplexedConnection;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor, manager::ManagerToken,
        shared::auth::LoginAsToken, spawn_await,
    },
    api_bail_not_found, api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
    signing::SignedData,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginAsReq {
    uid: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginAsRes {
    token: Box<str>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<LoginAsReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState {
        mut cache, db, key, ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let response =
        spawn_await(do_login_as(cache, db, key, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_login_as(
    mut cache: MultiplexedConnection,
    db: Database,
    key: SigningKey,
    token: ManagerToken,
    req: LoginAsReq,
) -> ApiResult<LoginAsRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify_sudo(&mut trans, true).await?;

        let exists: i64 = sqlx::query_scalar(include_str!(
            "./sqls/is_user_exists_by_uid.sql"
        ))
        .bind(req.uid)
        .fetch_one(&mut *trans)
        .await
        .context("is_user_exists_by_uid")?;
        if exists <= 0 {
            api_bail_not_found!("uid");
        }

        let token = LoginAsToken::new(&mut cache, req.uid)
            .await
            .context("LoginAsToken::new")?;
        let token = SignedData::sign(token, &key);
        let token = token.to_encoded();

        ApiResult::Ok(LoginAsRes { token })
    }
    .await;

    api::end_transaction(result, trans).await
}
