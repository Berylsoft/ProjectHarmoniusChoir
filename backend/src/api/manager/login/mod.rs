use anyhow::Context;
use argon2::PasswordHash;
use axum::{
    body::Body, extract::State, http::Response, response::IntoResponse,
};
use chrono::{DateTime, TimeDelta, Utc};
use ed25519_dalek::SigningKey;
use rand::RngCore;
use redis::aio::MultiplexedConnection;
use serde::{Deserialize, Serialize};
use sqlx::Transaction;
use ulid::Ulid;

use crate::{
    ServerState,
    api::{
        self, ApiError, ApiResult, ToCbor,
        manager::{
            ManagerToken, totp_check, totp_new, verify_password,
            verify_totp,
        },
        spawn_await, verify_nonce,
    },
    api_begin_transaction, api_param_assert,
    database::Database,
    extractors::Cbor,
    signing::{IntoSigned, SignedData},
    utils::cookie_set_token,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum LoginReq {
    Start {
        mid: i64,
        #[serde(with = "serde_bytes")]
        password: [u8; 64],
    },
    EndSetup {
        token: SignedData<TotpSetupToken>,
        totp_code: u32,
    },
    End {
        token: SignedData<LoginToken>,
        totp_code: u32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LoginRes {
    TotpSetup {
        token: SignedData<TotpSetupToken>,
        totp_url: String,
    },
    TotpVerify {
        token: SignedData<LoginToken>,
    },
    Success,
    InvalidCredential,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TotpSetupToken {
    #[serde(with = "serde_bytes")]
    secret: [u8; 20],
    login: LoginToken,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginToken {
    mid: i64,
    /// verified in `verify`
    rev: i64,
    /// verified in `pre_verify`
    nonce: Ulid,
    /// verified in `pre_verify`
    #[serde(with = "chrono::serde::ts_seconds")]
    expired: DateTime<Utc>,
}

impl LoginToken {
    async fn pre_verify(
        &self,
        cache_conn: &mut MultiplexedConnection,
    ) -> ApiResult<(), ToCbor> {
        if self.expired < Utc::now() {
            return Err(ApiError::InvalidToken("expired"));
        }

        verify_nonce(cache_conn, self.nonce).await?;

        Ok(())
    }

    /// # Return
    /// current `token_id`
    ///
    /// # Errors
    /// [`ApiError::Unknown`] when precondition "valid mid" unsatisfied or database error
    /// [`ApiError::InvalidToken`] when manager revision changed
    async fn verify(
        &self,
        trans: &mut Transaction<'_, sqlx::Sqlite>,
    ) -> ApiResult<i64, ToCbor> {
        let rev = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_manager_revision.sql"
        ))
        .bind(self.mid)
        .fetch_one(&mut **trans)
        .await
        .context("get_manager_revision")?;

        if rev != self.rev {
            return Err(ApiError::InvalidToken("manager updated"));
        }

        let token_id = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_manager_token_id.sql"
        ))
        .bind(self.mid)
        .fetch_one(&mut **trans)
        .await
        .context("get_manager_token_id")?;

        Ok(token_id)
    }
}

#[expect(clippy::cognitive_complexity)]
pub(crate) async fn router(
    state: State<ServerState>,
    req: Cbor<api::Request<LoginReq>>,
) -> api::ApiResult<Response<Body>, ToCbor> {
    let ServerState {
        key, db, mut cache, ..
    } = state.0;

    let req = req.0.verified(&mut cache).await?;

    // TODO: PoW rate limit
    let response = match req {
        LoginReq::Start { mid, password } => {
            api_param_assert!(!mid.is_negative());
            handle_start_login(db, mid, password, key).await?
        }
        LoginReq::EndSetup { token, totp_code } => {
            tracing::debug!("end login with totp setup");
            let token = token
                .verify(&key.verifying_key())
                .map_err(|_| ApiError::InvalidToken("signature"))?;
            token.login.pre_verify(&mut cache).await?;

            let valid = totp_check(token.secret.to_vec(), totp_code)?;
            if !valid {
                tracing::debug!("invalid totp code");
                return Ok(Cbor(api::Response::Ok(
                    LoginRes::InvalidCredential,
                ))
                .into_response());
            }

            let mid = token.login.mid;
            let token_id = spawn_await(end_setup(db, token)).await??;

            login_finish_res(mid, token_id, &key)
        }
        LoginReq::End { token, totp_code } => {
            tracing::debug!("end login");
            let token = token
                .verify(&key.verifying_key())
                .map_err(|_| ApiError::InvalidToken("signature"))?;
            token.pre_verify(&mut cache).await?;

            let mid = token.mid;
            let result =
                spawn_await(end_login(db, token, totp_code)).await??;
            match result {
                Ok(token_id) => login_finish_res(mid, token_id, &key),
                Err(response) => {
                    Cbor(api::Response::Ok(response)).into_response()
                }
            }
        }
    };

    Ok(response)
}

async fn handle_start_login(
    db: Database,
    mid: i64,
    password: [u8; 64],
    key: SigningKey,
) -> ApiResult<Response<Body>, ToCbor> {
    tracing::debug!("start login");

    let manager =
        spawn_await(get_manager_for_start_login(db, mid)).await??;
    let Some((revision, pswd, totp_secret)) = manager else {
        tracing::debug!("manager {mid} not found");
        return Ok(Cbor(api::Response::Ok(LoginRes::InvalidCredential))
            .into_response());
    };

    let pswd = PasswordHash::new(&pswd)
        .context("expect stored password is valid encoding")?;
    let valid = verify_password(password, &pswd).await?;
    if !valid {
        tracing::debug!("invalid password");
        return Ok(Cbor(api::Response::Ok(LoginRes::InvalidCredential))
            .into_response());
    }

    let token = LoginToken {
        mid,
        rev: revision,
        nonce: Ulid::new(),
        expired: Utc::now() + TimeDelta::minutes(10),
    };

    let login_res = if totp_secret.is_some() {
        LoginRes::TotpVerify {
            token: token.into_signed(&key),
        }
    } else {
        let mut secret = [0u8; 20];
        rand::rng().fill_bytes(&mut secret);
        let totp = totp_new(secret.to_vec(), mid);

        LoginRes::TotpSetup {
            token: TotpSetupToken {
                secret,
                login: token,
            }
            .into_signed(&key),
            totp_url: totp.get_url(),
        }
    };

    Ok(Cbor(api::Response::Ok(login_res)).into_response())
}

fn login_finish_res(
    mid: i64,
    token_id: i64,
    key: &SigningKey,
) -> axum::http::Response<axum::body::Body> {
    (
        [cookie_set_token(
            ManagerToken {
                mid,
                token_id,
                expired: Utc::now() + TimeDelta::days(7),
                sudo_expired: Utc::now() - TimeDelta::days(365),
            },
            key,
        )],
        Cbor(api::Response::Ok(LoginRes::Success)),
    )
        .into_response()
}

async fn get_manager_for_start_login(
    db: Database,
    mid: i64,
) -> ApiResult<Option<(i64, String, Option<Vec<u8>>)>, ToCbor> {
    let mut conn = db.acquire().await.context("acquire connection")?;

    let manager = sqlx::query_as::<_, (i64, String, Option<Vec<u8>>)>(
        include_str!("./sqls/get_manager_for_start_login.sql"),
    )
    .bind(mid)
    .fetch_optional(&mut *conn)
    .await
    .context("get_manager_for_start_login")?;

    Ok(manager)
}

async fn end_setup(
    db: Database,
    token: TotpSetupToken,
) -> ApiResult<i64, ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let res: ApiResult<_, ToCbor> = async {
        let token_id = token.login.verify(&mut trans).await?;

        sqlx::query(include_str!("./sqls/set_manager_totp_secret.sql"))
            .bind(&token.secret[..])
            .bind(token.login.mid)
            .execute(&mut *trans)
            .await
            .context("set_manager_totp_secret")?;

        Ok(token_id)
    }
    .await;

    api::end_transaction(res, trans).await
}

async fn end_login(
    db: Database,
    token: LoginToken,
    totp_code: u32,
) -> ApiResult<Result<i64, LoginRes>, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let res: ApiResult<_, ToCbor> = async {
        let token_id = token.verify(&mut trans).await?;

        let valid = verify_totp(&mut trans, token.mid, totp_code).await?;

        if !valid {
            tracing::debug!("invalid totp code");
            return Ok(Err(LoginRes::InvalidCredential));
        }

        Ok(Ok(token_id))
    }
    .await;

    api::end_transaction(res, trans).await
}
