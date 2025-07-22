use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use strum::{EnumString, IntoStaticStr};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    IntoStaticStr,
    EnumString,
    strum::Display,
)]
pub enum Status {
    Rejected,
    Passed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreSubmitStatus {
    Rejected { reason: PreSubmitRejectReason },
    Passed(GroupInfo),
}

impl TryFrom<PreSubmitReviewRow> for PreSubmitStatus {
    type Error = anyhow::Error;

    fn try_from(value: PreSubmitReviewRow) -> Result<Self, Self::Error> {
        let status: Status = value
            .status
            .parse()
            .context("parse Status from db result")?;

        Ok(match status {
            Status::Rejected => Self::Rejected {
                reason: value
                    .reason
                    .context("get reason when rejected")?
                    .parse()
                    .context("parse reason from db result")?,
            },
            Status::Passed => {
                let (Some(lead), Some(choir), Some(harmony)) =
                    (value.lead, value.choir, value.harmony)
                else {
                    anyhow::bail!("get group info when passed");
                };
                Self::Passed(GroupInfo {
                    lead,
                    choir,
                    harmony,
                })
            }
        })
    }
}

#[derive(Debug, FromRow)]
pub struct PreSubmitReviewRow {
    pub status: String,
    pub lead: Option<bool>,
    pub choir: Option<bool>,
    pub harmony: Option<bool>,
    pub reason: Option<String>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, FromRow,
)]
pub struct GroupInfo {
    pub lead: bool,
    pub choir: bool,
    pub harmony: bool,
}

impl GroupInfo {
    #[must_use]
    pub const fn new(lead: bool, choir: bool, harmony: bool) -> Self {
        Self {
            lead,
            choir,
            harmony,
        }
    }

    /// expect project user by `puid` exists and passed pre-submit
    /// # Returns
    /// None if project user not exists
    /// # Errors
    /// database errors
    pub async fn get_by_puid(
        trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        puid: i64,
    ) -> anyhow::Result<Self> {
        sqlx::query_as(include_str!("./sqls/get_group_info_by_puid.sql"))
            .bind(puid)
            .fetch_one(&mut **trans)
            .await
            .context("get_group_info_by_puid")
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    IntoStaticStr,
    EnumString,
    strum::Display,
    Serialize,
    Deserialize,
)]
pub enum PreSubmitRejectReason {
    DeviceOrEnvironment,
    RequirementNotMet,
    InvalidName,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SubmitStatus {
    Rejected {
        reason: PreSubmitRejectReason,
        detail: Option<Box<str>>,
    },
    Passed,
}

impl TryFrom<SubmitReviewRow> for SubmitStatus {
    type Error = anyhow::Error;

    fn try_from(value: SubmitReviewRow) -> Result<Self, Self::Error> {
        let status: Status = value
            .status
            .parse()
            .context("parse Status from db result")?;

        Ok(match status {
            Status::Rejected => Self::Rejected {
                reason: value
                    .reason
                    .context("get reason when rejected")?
                    .parse()
                    .context("parse reason from db result")?,
                detail: value.reason_detail.map(String::into_boxed_str),
            },
            Status::Passed => Self::Passed,
        })
    }
}

#[derive(Debug, FromRow)]
pub struct SubmitReviewRow {
    pub status: String,
    pub reason: Option<String>,
    pub reason_detail: Option<String>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    IntoStaticStr,
    EnumString,
    strum::Display,
    Serialize,
    Deserialize,
)]
pub enum SubmitRejectReason {
    DeviceOrEnvironment,
    RequirementNotMet,
    Other,
}
