use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, Response, ToJson, shared::auth::LoginAsToken,
        user::UserToken,
    },
    api_begin_transaction,
    database::Database,
    signing::SignedData,
    utils::cookie_set_token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginAsReq {
    token: Box<str>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    req: Json<api::Request<LoginAsReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState {
        mut cache, db, key, ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let uid = {
        let token =
            SignedData::<LoginAsToken>::try_from_encoded(&*req.token);
        let Ok(token) = token else {
            return Err(api::ApiError::InvalidToken(
                "encoding/serializing",
            ));
        };
        let token = token.verify(&key.verifying_key());
        let Ok(token) = token else {
            return Err(api::ApiError::InvalidToken("signaure"));
        };
        token.verify(&mut cache).await?;
        token.uid
    };

    let token_id = api::spawn_await(do_login_as(db, uid)).await??;

    Ok((
        [cookie_set_token(
            UserToken {
                uid,
                token_id,
                expired: Utc::now() + TimeDelta::days(1),
            },
            &key,
        )],
        Json(Response::Ok(())),
    ))
}

async fn do_login_as(db: Database, uid: i64) -> ApiResult<i64, ToJson> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        let token_id = sqlx::query_scalar(include_str!(
            "./sqls/get_token_id_by_uid.sql"
        ))
        .bind(uid)
        .fetch_one(&mut *trans)
        .await
        .context("get_token_id_by_uid")?;

        ApiResult::Ok(token_id)
    }
    .await;

    api::end_transaction(result, trans).await
}
