use std::sync::LazyLock;

use anyhow::{Context, ensure};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::SaltString,
};
use chrono::{DateTime, Utc};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use sqlx::Transaction;
use tokio::sync::Semaphore;

use crate::{
    ServerState,
    api::{ApiError, ApiResult},
    api_begin_transaction,
    database::try_end_transaction,
};

pub mod create_project;
pub mod login;

const ROOT_MID: i64 = 0;
const ROOT_DEFAULT_PASSWORD_LEN: usize = 32;
// TODO: actual parameter for production server
static ARGON2: LazyLock<argon2::Argon2> = LazyLock::new(|| {
    let params = argon2::Params::new(2 * 1024, 2, 1, Some(64)).unwrap();
    Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    )
});
static ARGON2_PARALLEL_SEMAPHORE: Semaphore = Semaphore::const_new(8);

#[derive(Debug, Serialize, Deserialize)]
pub struct ManagerToken {
    pub mid: i64,
    pub token_id: i64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expired: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub sudo_expired: DateTime<Utc>,
}

impl ManagerToken {
    async fn verify<S>(
        &self,
        trans: &mut Transaction<'_, sqlx::Any>,
    ) -> ApiResult<(), S> {
        if self.expired < Utc::now() {
            return Err(ApiError::InvalidToken("expired"));
        }

        let token_id = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_manager_token_id_by_mid.sql"
        ))
        .bind(self.mid)
        .fetch_optional(&mut **trans)
        .await
        .context("get_manager_token_id_by_mid")?;

        let token_id = token_id.ok_or(ApiError::InvalidToken(
            "manager not exists or deleted",
        ))?;

        if self.token_id != token_id {
            return Err(ApiError::InvalidToken("token revoked"));
        }

        Ok(())
    }

    async fn verify_root<S>(
        &self,
        trans: &mut Transaction<'_, sqlx::Any>,
    ) -> ApiResult<(), S> {
        self.verify(trans).await?;

        if self.mid != ROOT_MID {
            return Err(ApiError::InsufficientPermission("require root"));
        }

        Ok(())
    }

    async fn verify_sudo<S>(
        &self,
        trans: &mut Transaction<'_, sqlx::Any>,
        require_root: bool,
    ) -> ApiResult<(), S> {
        if require_root {
            self.verify_root(trans).await?;
        } else {
            self.verify(trans).await?;
        }

        if self.sudo_expired < Utc::now() {
            return Err(ApiError::RequireSudo);
        }

        Ok(())
    }
}

/// # Return
/// plain text password if initialized
/// # Errors
/// database error
/// # Panics
/// broken hash
pub async fn init_root_if_not_exists(
    state: &ServerState,
) -> anyhow::Result<Option<String>> {
    let mut password = String::with_capacity(ROOT_DEFAULT_PASSWORD_LEN);
    Alphanumeric.append_string(
        &mut rand::rng(),
        &mut password,
        ROOT_DEFAULT_PASSWORD_LEN,
    );
    let password_hash = Sha512::digest(password.as_bytes());
    let salt = SaltString::generate(&mut rand_core::OsRng);
    let password_hash = ARGON2
        .hash_password(&password_hash, &salt)
        .expect("valid hash");
    let password_hash = password_hash.serialize();

    api_begin_transaction!(state.db, conn, trans, Immediate);

    let res: anyhow::Result<_> = async {
        let exists = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_root.sql"
        ))
        .fetch_optional(&mut *trans)
        .await
        .context("get_root")?
        .is_some();

        if exists {
            return Ok(None);
        }

        let ins_result = sqlx::query(include_str!("./sqls/ins_root.sql"))
            .bind(password_hash.as_str())
            .execute(&mut *trans)
            .await
            .context("ins_root")?;
        ensure!(ins_result.rows_affected() == 1);

        Ok(Some(password))
    }
    .await;

    try_end_transaction(res, trans)
        .await
        .context("end_transaction")?
}

async fn verify_password<S>(
    pswd_sha512: [u8; 64],
    stored_password: &PasswordHash<'_>,
) -> ApiResult<(), S> {
    let permit = ARGON2_PARALLEL_SEMAPHORE
        .acquire()
        .await
        .expect("not closed");

    let stored_password = stored_password.serialize();
    let verify_res = tokio::task::spawn_blocking(move || {
        let result = ARGON2.verify_password(
            &pswd_sha512,
            &stored_password.password_hash(),
        );

        drop(permit);

        result
    })
    .await
    .context("failed to wait password verify to return")?;

    if verify_res == Err(argon2::password_hash::Error::Password) {
        return Err(ApiError::InvalidCredential("password"));
    }

    verify_res.context("verify password")?;
    Ok(())
}

// TODO: update password hash when parameter changed
