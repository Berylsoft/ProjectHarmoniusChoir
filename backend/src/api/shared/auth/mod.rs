use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use redis::{
    AsyncTypedCommands, ExistenceCheck, SetExpiry, SetOptions,
    aio::MultiplexedConnection,
};
use serde::{Deserialize, Serialize};
use ulid::{Ulid, serde::ulid_as_u128};

use crate::api::{ApiError, ApiResult};

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginAsToken {
    /// target user id
    pub uid: i64,
    /// for single use
    #[serde(with = "ulid_as_u128")]
    pub nonce: Ulid,
    /// the last time this token will be valid
    /// it may be invalid before this time
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expired: DateTime<Utc>,
}

impl LoginAsToken {
    pub(crate) async fn new(
        cache_conn: &mut MultiplexedConnection,
        uid: i64,
    ) -> anyhow::Result<Self> {
        let expired = Utc::now() + TimeDelta::minutes(2);

        let nonce = Ulid::new();
        let success = cache_conn
            .set_nx(format!("login_as:nonce:{nonce}"), "pending")
            .await
            .context("cache_conn.set_nx")?;
        if !success {
            anyhow::bail!("nonce collision");
        }

        Ok(Self {
            uid,
            nonce,
            expired,
        })
    }

    pub(crate) async fn verify<S>(
        &self,
        cache_conn: &mut MultiplexedConnection,
    ) -> ApiResult<(), S> {
        let now = Utc::now();
        if self.expired < now {
            return Err(ApiError::InvalidToken("expired"));
        }

        let ex_at =
            u64::try_from(self.expired.timestamp()).unwrap_or_default();
        let old = cache_conn
            .set_options(
                format!("login_as:nonce:{}", self.nonce),
                "used",
                SetOptions::default()
                    .conditional_set(ExistenceCheck::XX)
                    .get(true)
                    .with_expiration(SetExpiry::EXAT(ex_at)),
            )
            .await
            .context("cache_conn.set_options XX GET EXAT")?;

        let Some(old) = old else {
            return Err(ApiError::InvalidToken("nonce not found"));
        };

        if old != "pending" {
            return Err(ApiError::InvalidToken("nonce used"));
        }

        Ok(())
    }
}
