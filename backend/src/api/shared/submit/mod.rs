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
pub struct Detail<S> {
    pub mid: i64,
    pub mname: Box<str>,
    pub status: S,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreSubmitStatus {
    Rejected { reason: PreSubmitRejectReason },
    Passed(GroupInfo),
}

impl PreSubmitStatus {
    /// expect project user by `puid` exists and pre-submit is exists and not pending
    /// # Errors
    /// database errors
    pub async fn get_by_puid(
        trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        puid: i64,
    ) -> anyhow::Result<Self> {
        let row: PreSubmitReviewRow = sqlx::query_as(include_str!(
            "./sqls/get_pre_submit_status_by_puid.sql"
        ))
        .bind(puid)
        .fetch_one(&mut **trans)
        .await
        .context("get_pre_submit_status_by_puid")?;

        row.try_into()
            .context("PreSubmitReviewRow try_into PreSubmitStatus")
    }
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
                let (
                    Some(lead),
                    Some(choir),
                    Some(harmony),
                    Some(choir_harmony),
                ) = (
                    value.lead,
                    value.choir,
                    value.harmony,
                    value.choir_harmony,
                )
                else {
                    anyhow::bail!("get group info when passed");
                };
                Self::Passed(GroupInfo {
                    lead,
                    choir,
                    harmony,
                    choir_harmony,
                })
            }
        })
    }
}

#[derive(Debug, FromRow)]
pub struct PreSubmitReviewRow {
    pub status: Box<str>,
    pub lead: Option<bool>,
    pub choir: Option<bool>,
    pub harmony: Option<bool>,
    pub choir_harmony: Option<bool>,
    pub reason: Option<Box<str>>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, FromRow,
)]
#[expect(clippy::struct_excessive_bools)]
pub struct GroupInfo {
    pub lead: bool,
    pub choir: bool,
    pub harmony: bool,
    pub choir_harmony: bool,
}

impl GroupInfo {
    #[must_use]
    #[expect(clippy::fn_params_excessive_bools)]
    pub const fn new(
        lead: bool,
        choir: bool,
        harmony: bool,
        choir_harmony: bool,
    ) -> Self {
        Self {
            lead,
            choir,
            harmony,
            choir_harmony,
        }
    }

    /// expect project user by `puid` exists and passed pre-submit
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
        reason: SubmitRejectReason,
        detail: Option<Box<str>>,
    },
    Passed,
}

impl SubmitStatus {
    /// expect project user by `puid` exists and submit is exists and not pending
    /// # Errors
    /// database errors
    pub async fn get_by_puid(
        trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        puid: i64,
    ) -> anyhow::Result<Self> {
        let row: SubmitReviewRow = sqlx::query_as(include_str!(
            "./sqls/get_submit_status_by_puid.sql"
        ))
        .bind(puid)
        .fetch_one(&mut **trans)
        .await
        .context("get_submit_status_by_puid")?;

        row.try_into()
            .context("SubmitReviewRow try_into SubmitStatus")
    }
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
                detail: value.reason_detail,
            },
            Status::Passed => Self::Passed,
        })
    }
}

#[derive(Debug, FromRow)]
pub struct SubmitReviewRow {
    pub status: Box<str>,
    pub reason: Option<Box<str>>,
    pub reason_detail: Option<Box<str>>,
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
