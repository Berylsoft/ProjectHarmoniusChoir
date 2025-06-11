use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod wechat_login_or_register;

#[derive(Debug, Serialize, Deserialize)]
pub struct UserToken {
    pub uid: i64,
    pub token_id: i64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expired: DateTime<Utc>,
}
