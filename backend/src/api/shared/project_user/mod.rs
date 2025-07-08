use std::str::FromStr as _;

use anyhow::Context as _;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::shared::submit;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
pub enum Status {
    Entered,
    PreSubmitted,
    PreSubmitRejected,
    PreSubmitPassed,
    Submitted,
    SubmitRejected,
    SubmitPassed,
    // manager only
    Mastered,
    Mixed,
}

impl Status {
    /// # Returns
    /// the status and the time it was reached
    /// # Errors
    /// anything unexpected
    #[expect(clippy::too_many_lines)]
    pub async fn get_by_puid(
        trans: &mut sqlx::Transaction<'_, sqlx::Any>,
        puid: i64,
    ) -> anyhow::Result<(Self, DateTime<Utc>)> {
        fn parse_time(
            time: &str,
            name: &str,
        ) -> anyhow::Result<DateTime<Utc>> {
            Ok(DateTime::parse_from_rfc3339(time)
                .with_context(|| format!("parse time: {name}"))?
                .to_utc())
        }

        let pre_submit =
            sqlx::query_as::<_, (i64, String)>(include_str!(
                "./sqls/get_project_user_latest_pre_submit_by_puid.sql"
            ))
            .bind(puid)
            .fetch_optional(&mut **trans)
            .await
            .context("get_project_user_latest_pre_submit_by_puid")?;

        let Some((submit_id, submit_at)) = pre_submit else {
            let joined_at =
                sqlx::query_scalar::<_, String>(include_str!(
                    "./sqls/get_project_user_joined_at_by_puid.sql"
                ))
                .bind(puid)
                .fetch_one(&mut **trans)
                .await
                .context("get_project_user_joined_at_by_puid")?;
            return Ok((
                Self::Entered,
                parse_time(&joined_at, "user joined_at")?,
            ));
        };

        let review_status = sqlx::query_as::<_, (String, String)>(
            include_str!("./sqls/get_pre_submit_review_by_psid.sql"),
        )
        .bind(submit_id)
        .fetch_optional(&mut **trans)
        .await
        .context("get_pre_submit_review_by_psid")?;

        let Some((review_status, review_at)) = review_status else {
            return Ok((
                Self::PreSubmitted,
                parse_time(&submit_at, "latest pre submit at")?,
            ));
        };

        let review_status = submit::Status::from_str(&review_status)
            .context(
                "submit::Status::from_str(&pre_submit_review_status)",
            )?;

        let submit = sqlx::query_as::<_, (i64, String)>(include_str!(
            "./sqls/get_project_user_latest_submit_by_puid.sql"
        ))
        .bind(puid)
        .fetch_optional(&mut **trans)
        .await
        .context("get_project_user_latest_submit_by_puid")?;

        let Some((submit_id, submit_at)) = submit else {
            return Ok((
                match review_status {
                    submit::Status::Rejected => Self::PreSubmitRejected,
                    submit::Status::Passed => Self::PreSubmitPassed,
                },
                parse_time(&review_at, "pre submit review at")?,
            ));
        };

        let review_status = sqlx::query_as::<_, (String, String)>(
            include_str!("./sqls/get_submit_review_by_sid.sql"),
        )
        .bind(submit_id)
        .fetch_optional(&mut **trans)
        .await
        .context("get_submit_review_by_sid")?;

        let Some((review_status, review_at)) = review_status else {
            return Ok((
                Self::Submitted,
                parse_time(&submit_at, "latest submit at")?,
            ));
        };

        let review_status = submit::Status::from_str(&review_status)
            .context("submit::Status::from_str(&submit_review_status)")?;

        let master_at = sqlx::query_scalar::<_, String>(include_str!(
            "./sqls/get_master_at_by_puid.sql"
        ))
        .bind(puid)
        .fetch_optional(&mut **trans)
        .await
        .context("get_master_at_by_puid")?;

        let Some(mastered_at) = master_at else {
            return Ok((
                match review_status {
                    submit::Status::Rejected => Self::SubmitRejected,
                    submit::Status::Passed => Self::SubmitPassed,
                },
                parse_time(&review_at, "submit review at")?,
            ));
        };

        let mixed_at = sqlx::query_scalar::<_, String>(include_str!(
            "./sqls/get_mixed_at_by_puid.sql"
        ))
        .bind(puid)
        .fetch_optional(&mut **trans)
        .await
        .context("get_mixed_at_by_puid")?;

        if let Some(mixed_at) = mixed_at {
            Ok((Self::Mixed, parse_time(&mixed_at, "mixed at")?))
        } else {
            Ok((Self::Mastered, parse_time(&mastered_at, "mastered at")?))
        }
    }
}
