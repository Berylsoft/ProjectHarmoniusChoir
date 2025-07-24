use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{api::ApiResult, api_bail_status};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub name: Box<str>,
    pub require_harmony_group_intention: bool,
    pub pre_submit_file_size_min: i64,
    pub pre_submit_file_size_max: i64,
    pub submit_file_size_min: i64,
    pub submit_file_size_max: i64,
    pub end_time: DateTime<Utc>,
}

impl Info {
    /// # Errors
    /// database errors
    pub async fn get_by_id(
        trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        id: i64,
    ) -> anyhow::Result<Option<Self>> {
        let row: Option<InfoRow> = sqlx::query_as(include_str!(
            "./sqls/get_project_info_by_id.sql"
        ))
        .bind(id)
        .fetch_optional(&mut **trans)
        .await
        .context("get_project_info_by_id")?;

        row.map(TryInto::try_into).transpose()
    }
}

impl TryFrom<InfoRow> for Info {
    type Error = anyhow::Error;

    fn try_from(value: InfoRow) -> Result<Self, Self::Error> {
        let InfoRow {
            require_harmony_group_intention,
            pre_submit_file_size_min,
            pre_submit_file_size_max,
            submit_file_size_min,
            submit_file_size_max,
            ..
        } = value;

        Ok(Self {
            name: value.name.into_boxed_str(),
            require_harmony_group_intention,
            pre_submit_file_size_min,
            pre_submit_file_size_max,
            submit_file_size_min,
            submit_file_size_max,
            end_time: value
                .end_time
                .parse()
                .context("end_time.parse()")?,
        })
    }
}

#[derive(Debug, Clone, FromRow)]
struct InfoRow {
    name: String,
    require_harmony_group_intention: bool,
    pre_submit_file_size_min: i64,
    pre_submit_file_size_max: i64,
    submit_file_size_min: i64,
    submit_file_size_max: i64,
    end_time: String,
}

/// expect project `id` exists
/// # Errors
/// database errors
pub async fn get_attachment_key_by_id(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> anyhow::Result<Option<Box<str>>> {
    let key: Option<String> = sqlx::query_scalar(include_str!(
        "./sqls/get_attachment_key_by_id.sql"
    ))
    .bind(id)
    .fetch_optional(&mut **trans)
    .await
    .context("get_attachment_key_by_id")?;

    Ok(key.map(String::into_boxed_str))
}

/// expect project `id` exists
/// # Errors
/// when project `id` is actually not exists
/// or other database errors
pub async fn is_ended_by_id(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> anyhow::Result<bool> {
    let end_time: String =
        sqlx::query_scalar(include_str!("./sqls/get_end_time_by_id.sql"))
            .bind(id)
            .fetch_one(&mut **trans)
            .await
            .context("get_end_time_by_id")?;

    let end_time: DateTime<Utc> =
        end_time.parse().context("expect valid rfc3339")?;

    Ok(end_time < Utc::now())
}

pub(crate) async fn verify_not_ended<S>(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> ApiResult<(), S> {
    if is_ended_by_id(trans, id)
        .await
        .context("project::is_ended_by_id")?
    {
        api_bail_status!("project ended");
    }

    Ok(())
}
