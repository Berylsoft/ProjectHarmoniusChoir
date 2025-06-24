use std::sync::LazyLock;

use anyhow::{Context, ensure};
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
use chrono::{DateTime, Utc};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use sqlx::Transaction;

use crate::{
    ServerState,
    api::{ApiError, ApiResult},
    api_begin_transaction,
    database::try_end_transaction,
};

const ROOT_MID: i64 = 0;
const ROOT_DEFAULT_PASSWORD_LEN: usize = 32;

// TODO: actual parameter for production server
pub static ARGON2: LazyLock<argon2::Argon2> = LazyLock::new(|| {
    let params = argon2::Params::new(2 * 1024, 2, 1, Some(64)).unwrap();
    Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    )
});

#[derive(Debug, Serialize, Deserialize)]
pub struct ManagerToken {
    pub mid: i64,
    pub token_id: i64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expired: DateTime<Utc>,
}

impl ManagerToken {
    pub async fn verify<S>(
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
}

/// # Return
/// plain text password if initialized
pub async fn init_root_if_not_exists(
    state: &ServerState,
) -> anyhow::Result<Option<String>> {
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

        let mut password =
            String::with_capacity(ROOT_DEFAULT_PASSWORD_LEN);
        Alphanumeric.append_string(
            &mut rand::rng(),
            &mut password,
            ROOT_DEFAULT_PASSWORD_LEN,
        );
        let password_hash = Sha512::digest(password.as_bytes());
        let salt = SaltString::generate(&mut rand_core::OsRng);
        let password_hash =
            ARGON2.hash_password(&password_hash, &salt).unwrap();
        let password_hash = password_hash.serialize();

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
        .context("init_root_if_not_exists")
}
