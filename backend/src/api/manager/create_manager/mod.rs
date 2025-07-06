use anyhow::Context as _;
use argon2::password_hash::PasswordHashString;
use axum::{extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::{ManagerToken, generate_default_password},
        spawn_await,
    },
    api_assert, api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateManagerRes {
    mid: i64,
    password: Box<str>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<()>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    req.0.verified(&mut cache).await?;

    let (password, password_hash) = generate_default_password()
        .await
        .context("generate_default_password")?;

    let mid = spawn_await(do_create_manager(db, token.0, password_hash))
        .await??;

    Ok(Cbor(api::Response::Ok(CreateManagerRes { mid, password })))
}

async fn do_create_manager(
    db: Database,
    token: ManagerToken,
    password: PasswordHashString,
) -> ApiResult<i64, ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify_sudo(&mut trans, true).await?;

        let latest_mid = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_latest_mid.sql"
        ))
        .fetch_one(&mut *trans)
        .await
        .context("get_latest_mid")?;

        let mid = latest_mid + 1;

        api_assert!(mid >= 1);

        let ins_result =
            sqlx::query(include_str!("./sqls/ins_manager.sql"))
                .bind(mid)
                .bind(password.as_str())
                .execute(&mut *trans)
                .await
                .context("ins_manager")?;

        api_assert!(ins_result.rows_affected() == 1);

        ApiResult::Ok(mid)
    }
    .await;

    api::end_transaction(result, trans).await
}
