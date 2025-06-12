use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    api::{ApiError, ApiResult},
    database::Database,
};

pub mod update_name;
pub mod wechat_login_or_register;

#[derive(Debug, Serialize, Deserialize)]
pub struct UserToken {
    pub uid: i64,
    pub token_id: i64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expired: DateTime<Utc>,
}

impl UserToken {
    pub async fn verify<S>(&self, db: &Database) -> ApiResult<(), S> {
        if self.expired < Utc::now() {
            return Err(ApiError::InvalidToken("expired"));
        }

        let mut conn =
            db.acquire().await.context("db connection acquire")?;

        let token_id = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_user_token_id_by_uid.sql"
        ))
        .bind(self.uid)
        .fetch_optional(&mut *conn)
        .await
        .context("get_user_token_id_by_uid")?;

        let token_id = token_id.ok_or(ApiError::InvalidToken(
            "user not exists or deleted",
        ))?;

        if self.token_id != token_id {
            return Err(ApiError::InvalidToken("token revoked"));
        }

        Ok(())
    }
}
