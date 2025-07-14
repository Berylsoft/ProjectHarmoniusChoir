use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Transaction;

use crate::api::{ApiError, ApiResult};

pub mod join_project;
pub mod list_projects;
pub mod pre_submit;
pub mod project_info;
pub mod revoke_all_tokens;
pub mod template;
pub mod update_name;
pub mod upload_file;
pub mod wechat_login_or_register;

#[derive(Debug, Serialize, Deserialize)]
pub struct UserToken {
    pub uid: i64,
    pub token_id: i64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expired: DateTime<Utc>,
}

impl UserToken {
    async fn verify<S>(
        &self,
        trans: &mut Transaction<'_, sqlx::Sqlite>,
    ) -> ApiResult<(), S> {
        if self.expired < Utc::now() {
            return Err(ApiError::InvalidToken("expired"));
        }

        let token_id = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_user_token_id_by_uid.sql"
        ))
        .bind(self.uid)
        .fetch_optional(&mut **trans)
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

    /// # Returns
    /// `project_user_id`
    async fn verify_joined_project<S>(
        &self,
        trans: &mut Transaction<'_, sqlx::Sqlite>,
        pid: i64,
    ) -> ApiResult<i64, S> {
        let project_user_id = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_project_user_id_by_uid_pid.sql"
        ))
        .bind(self.uid)
        .bind(pid)
        .fetch_optional(&mut **trans)
        .await
        .context("get_project_user_id_by_uid_pid")?;

        let Some(project_user_id) = project_user_id else {
            return Err(ApiError::InsufficientPermission(
                "have not joined this project",
            ));
        };

        Ok(project_user_id)
    }
}
