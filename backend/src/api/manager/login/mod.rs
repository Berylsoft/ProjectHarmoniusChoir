use anyhow::Context;
use argon2::PasswordHash;
use axum::{
    body::Body, extract::State, http::Response, response::IntoResponse,
};
use chrono::{DateTime, TimeDelta, Utc};
use rand::RngCore;
use redis::aio::MultiplexedConnection;
use serde::{Deserialize, Serialize};
use sqlx::Transaction;
use totp_rs::TOTP;
use ulid::Ulid;

use crate::{
    ServerState,
    api::{
        self, ApiError, ApiResult, ToCbor,
        manager::{ManagerToken, verify_password},
        spawn_await, verify_nonce,
    },
    api_begin_transaction,
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
        trans: &mut Transaction<'_, sqlx::Any>,
    ) -> ApiResult<i64, ToCbor> {
        let rev = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_manager_revision.sql"
        ))
        .fetch_one(&mut **trans)
        .await
        .context("get_manager_revision")?;

        if rev != self.rev {
            return Err(ApiError::InvalidToken("manager updated"));
        }

        let token_id = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_manager_token_id.sql"
        ))
        .fetch_one(&mut **trans)
        .await
        .context("get_manager_token_id")?;

        Ok(token_id)
    }
}

pub(crate) async fn router(
    state: State<ServerState>,
    req: Cbor<api::Request<LoginReq>>,
) -> api::ApiResult<Response<Body>, ToCbor> {
    let ServerState { key, db, mut cache } = state.0;

    let req = req.0.verified(&mut cache).await?;

    let login_finish = |mid: i64, token_id: i64| {
        Ok((
            [cookie_set_token(
                ManagerToken {
                    mid,
                    token_id,
                    expired: Utc::now() + TimeDelta::days(7),
                },
                &key,
            )],
            Cbor(api::Response::Ok(())),
        )
            .into_response())
    };

    // TODO: PoW rate limit
    match req {
        LoginReq::Start { mid, password } => {
            tracing::debug!("start login");

            let manager =
                spawn_await(get_manager_for_start_login(db, mid))
                    .await??;
            let Some((revision, pswd, totp_secret)) = manager else {
                return Err(ApiError::InvalidCredential(
                    "unknown manager",
                ));
            };

            let pswd = PasswordHash::new(&pswd)
                .context("expect stored password is valid encoding")?;
            verify_password(&password, &pswd).await?;

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
                let totp = totp_new(mid, secret.to_vec());

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
        LoginReq::EndSetup { token, totp_code } => {
            tracing::debug!("end login with totp setup");
            let token = token
                .verify(&key.verifying_key())
                .map_err(|_| ApiError::InvalidToken("signature"))?;
            token.login.pre_verify(&mut cache).await?;

            totp_check(
                token.login.mid,
                token.secret.to_vec(),
                totp_code,
            )?;

            let mid = token.login.mid;
            let token_id = spawn_await(end_setup(db, token)).await??;

            login_finish(mid, token_id)
        }
        LoginReq::End { token, totp_code } => {
            tracing::debug!("end login");
            let token = token
                .verify(&key.verifying_key())
                .map_err(|_| ApiError::InvalidToken("signature"))?;
            token.pre_verify(&mut cache).await?;

            let mid = token.mid;
            let token_id =
                spawn_await(end_login(db, token, totp_code)).await??;

            login_finish(mid, token_id)
        }
    }
}

fn totp_new(mid: i64, secret: Vec<u8>) -> TOTP {
    TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        secret,
        Some("LuminizorsPassportForAdmins".to_string()),
        mid.to_string(),
    )
    .unwrap()
}
fn totp_check(
    mid: i64,
    secret: Vec<u8>,
    totp_code: u32,
) -> ApiResult<(), ToCbor> {
    totp_new(mid, secret)
        .check_current(&totp_code.to_string())
        .context("failed to get system time")?
        .then_some(())
        .ok_or(ApiError::InvalidCredential("invalid totp_code"))
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
) -> ApiResult<i64, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let res: ApiResult<_, ToCbor> = async {
        let token_id = token.verify(&mut trans).await?;

        let totp_secret = sqlx::query_scalar::<_, Vec<u8>>(include_str!(
            "./sqls/get_manager_totp_secret.sql"
        ))
        .bind(token.mid)
        .fetch_one(&mut *trans)
        .await
        .context("get_manager_totp_secret")?;

        totp_check(token.mid, totp_secret, totp_code)?;

        Ok(token_id)
    }
    .await;

    api::end_transaction(res, trans).await
}
