use std::{path::Path, time::Duration};

use anyhow::Context as _;
use aws_sdk_s3::{
    operation::head_object::HeadObjectError,
    presigning::{PresignedRequest, PresigningConfig},
};
use base64::{Engine, prelude::BASE64_STANDARD};
use itertools::Itertools as _;
use serde::{Deserialize, Serialize};
use strum::{EnumString, IntoStaticStr};

use crate::{S3, api::shared::project_user};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresignedReq {
    method: Box<str>,
    uri: Box<str>,
    headers: Vec<(Box<str>, Box<str>)>,
}

impl From<PresignedRequest> for PresignedReq {
    fn from(value: PresignedRequest) -> Self {
        Self {
            method: value.method().into(),
            uri: value.uri().into(),
            headers: value
                .headers()
                .map(|(k, v)| (k.into(), v.into()))
                .collect_vec(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub id: i64,
    pub name: Box<str>,
}

impl Info {
    /// expect `sid` valid
    /// # Errors
    /// database error
    pub async fn get_all_of_submit_by_sid(
        trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        sid: i64,
    ) -> anyhow::Result<Vec<Self>> {
        let files: Vec<(i64, String)> = sqlx::query_as(include_str!(
            "./sqls/get_file_infos_of_submit_by_sid.sql"
        ))
        .bind(sid)
        .fetch_all(&mut **trans)
        .await
        .context("get_file_infos_of_submit_by_sid")?;

        Ok(files
            .into_iter()
            .map(|(id, name)| Self {
                id,
                name: name.into_boxed_str(),
            })
            .collect_vec())
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    IntoStaticStr,
    EnumString,
)]
pub enum Stage {
    PreSubmit,
    Submit,
    Master,
}

impl Stage {
    #[must_use]
    pub fn into_str(self) -> &'static str {
        self.into()
    }

    #[must_use]
    pub const fn is_pre_submit(&self) -> bool {
        matches!(self, Self::PreSubmit)
    }

    #[must_use]
    pub const fn is_submit(&self) -> bool {
        matches!(self, Self::Submit)
    }

    #[must_use]
    pub const fn is_master(&self) -> bool {
        matches!(self, Self::Master)
    }
}

impl TryFrom<project_user::Status> for Stage {
    type Error = ();

    fn try_from(
        value: project_user::Status,
    ) -> Result<Self, Self::Error> {
        match value {
            project_user::Status::Entered
            | project_user::Status::PreSubmitRejected => {
                Ok(Self::PreSubmit)
            }
            project_user::Status::PreSubmitPassed
            | project_user::Status::SubmitRejected => Ok(Self::Submit),
            project_user::Status::SubmitPassed => Ok(Self::Master),
            project_user::Status::PreSubmitted
            | project_user::Status::Submitted
            | project_user::Status::Mastered
            | project_user::Status::Mixed => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Source {
    project_id: i64,
    user_id: i64,
    manager_id: Option<i64>,
    stage: Stage,
}

impl Source {
    #[must_use]
    pub const fn new(
        project_id: i64,
        user_id: i64,
        manager_id: Option<i64>,
        stage: Stage,
    ) -> Self {
        Self {
            project_id,
            user_id,
            manager_id,
            stage,
        }
    }
}

/// expect `source` is valid
pub(crate) async fn upload_check_capacity(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source: Source,
    size: i64,
) -> anyhow::Result<bool> {
    const USER_DEFAULT_CAPACITY: i64 = 1_000_000_000;
    const MGR_DEFAULT_CAPACITY: i64 = 1_000_000_000;

    let enough = if let Some(mid) = source.manager_id {
        let file_capacity = sqlx::query_scalar::<_, Option<i64>>(
            include_str!("./sqls/get_manager_capacity_by_mid.sql"),
        )
        .bind(mid)
        .fetch_one(&mut **trans)
        .await
        .context("get_manager_capacity_by_mid")?;

        let file_capacity = file_capacity.unwrap_or(MGR_DEFAULT_CAPACITY);

        let unused_file_size =
            sqlx::query_scalar::<_, i64>(include_str!(
                "./sqls/get_manager_unused_file_size_by_mid.sql"
            ))
            .bind(mid)
            .fetch_one(&mut **trans)
            .await
            .context("get_manager_unused_file_size_by_mid")?;

        unused_file_size + size <= file_capacity
    } else {
        let file_capacity = sqlx::query_scalar::<_, Option<i64>>(
            include_str!("./sqls/get_user_capacity_by_uid.sql"),
        )
        .bind(source.user_id)
        .fetch_one(&mut **trans)
        .await
        .context("get_user_capacity_by_uid")?;

        let file_capacity =
            file_capacity.unwrap_or(USER_DEFAULT_CAPACITY);

        let unused_file_size = sqlx::query_scalar::<_, i64>(
            include_str!("./sqls/get_user_unused_file_size_by_uid.sql"),
        )
        .bind(source.user_id)
        .fetch_one(&mut **trans)
        .await
        .context("get_user_unused_file_size_by_uid")?;

        unused_file_size + size <= file_capacity
    };

    Ok(enough)
}

/// expect `source` is valid
pub(crate) async fn upload_check_pending_file_count(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source: Source,
    stage: Stage,
) -> anyhow::Result<bool> {
    let limit = match stage {
        Stage::Submit => 1000,
        Stage::Master | Stage::PreSubmit => 1,
    };

    let count = match (stage, source.manager_id) {
        (Stage::PreSubmit | Stage::Submit, None) => {
            sqlx::query_scalar::<_, i64>(include_str!(
                "./sqls/get_user_pending_file_count_by_pid_uid.sql"
            ))
            .bind(source.project_id)
            .bind(source.user_id)
            .fetch_one(&mut **trans)
            .await
            .context("get_user_pending_file_count_by_pid_uid")?
        }
        (Stage::Master, Some(mid)) => {
            sqlx::query_scalar::<_, i64>(include_str!(
                "./sqls/get_manager_pending_file_count_by_pid_uid_mid.sql"
            ))
            .bind(source.project_id)
            .bind(source.user_id)
            .bind(mid)
            .fetch_one(&mut **trans)
            .await
            .context("get_manager_pending_file_count_by_pid_uid_mid")?
        }
        _ => {
            anyhow::bail!("unreachable");
        }
    };

    Ok(count < limit)
}

/// expect capacity checked and `source` is valid
/// # Returns
/// `file_id` and the pre-signed s3 upload request
pub(crate) async fn upload_start(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    s3: &S3,
    source: Source,
    name: Box<str>,
    size: i64,
    md5: Box<[u8; 16]>,
    file_type: Type,
) -> anyhow::Result<(i64, PresignedReq)> {
    let (key, req) =
        presigned_upload_req(s3, source, size, &md5, file_type)
            .await
            .context("presigned_upload_req")?;

    let id =
        sqlx::query_scalar::<_, i64>(include_str!("./sqls/ins_file.sql"))
            .bind(source.project_id)
            .bind(source.user_id)
            .bind(source.manager_id)
            .bind(source.stage.into_str())
            .bind(&*name)
            .bind(&*key)
            .bind(size)
            .bind(md5.as_slice())
            .bind(file_type.to_mime_str())
            .fetch_one(&mut **trans)
            .await
            .context("ins_file")?;

    Ok((id, req))
}

/// # Returns
/// None if `file_id` not exists, false if file not finished uploading
pub(crate) async fn upload_finish(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    s3: &S3,
    file_id: i64,
    user_id: i64,
    manager_id: Option<i64>,
) -> anyhow::Result<Option<bool>> {
    let key = sqlx::query_scalar::<_, String>(include_str!(
        "./sqls/get_file_s3_key_by_id_uid_mid.sql"
    ))
    .bind(file_id)
    .bind(manager_id)
    .bind(user_id)
    .bind(manager_id)
    .fetch_optional(&mut **trans)
    .await
    .context("get_file_s3_key_by_id_uid_mid")?;

    let Some(key) = key else { return Ok(None) };

    let is_pending = is_pending_by_id(trans, file_id).await?;
    if is_pending {
        return Ok(Some(true));
    }

    let exists = is_s3_file_exists(s3, key)
        .await
        .context("is_s3_file_exists")?;

    if !exists {
        return Ok(Some(false));
    }

    let ins_res = sqlx::query(include_str!("./sqls/ins_pending.sql"))
        .bind(file_id)
        .execute(&mut **trans)
        .await
        .context("ins_pending")?;

    anyhow::ensure!(ins_res.rows_affected() == 1);

    Ok(Some(true))
}

/// # Returns
/// `key` and the pre-signed s3 upload request
async fn presigned_upload_req(
    s3: &S3,
    source: Source,
    size: i64,
    md5: &[u8; 16],
    file_type: Type,
) -> anyhow::Result<(Box<str>, PresignedReq)> {
    let key = format!(
        "/uploads/{project}/{user}/{stage}/{md5}",
        project = source.project_id,
        user = source.user_id,
        stage = source.stage.into_str(),
        md5 = hex::encode(md5.as_slice()),
    )
    .into_boxed_str();
    let md5 = BASE64_STANDARD.encode(md5.as_slice());

    let req = s3
        .put_object()
        .bucket(&*s3.bucket)
        .key(&*key)
        .content_md5(md5)
        .content_length(size)
        .content_type(file_type.to_mime_str())
        .presigned(
            PresigningConfig::builder()
                .expires_in(Duration::from_secs(30))
                .build()
                .expect("valid expires_in"),
        )
        .await
        .context("presigning put_object for upload")?;

    Ok((key, req.into()))
}

pub(crate) async fn is_s3_file_exists(
    s3: &S3,
    key: impl Into<String>,
) -> anyhow::Result<bool> {
    let result =
        s3.head_object().bucket(&*s3.bucket).key(key).send().await;

    if let Err(ref err) = result
        && let Some(HeadObjectError::NotFound(_)) = err.as_service_error()
    {
        return Ok(false);
    }

    result.context("head_object")?;

    Ok(true)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Wav,
    Flac,
    Ogg,
    Mp3,
    M4a,
    Aac,
}

impl Type {
    #[must_use]
    pub fn detect(head: &[u8; 12]) -> Option<Self> {
        if &head[..4] == b"RIFF" && &head[8..12] == b"WAVE" {
            Some(Self::Wav)
        } else if &head[..4] == b"fLaC" {
            Some(Self::Flac)
        } else if &head[..4] == b"OggS" {
            Some(Self::Ogg)
        } else if &head[..3] == b"ID3"
            || (head[..2] == [0xff, 0xfb]
                || head[..2] == [0xff, 0xf3]
                || head[..2] == [0xff, 0xf2])
        {
            Some(Self::Mp3)
        } else if &head[4..12] == b"ftypM4A " {
            Some(Self::M4a)
        } else if head[..2] == [0xff, 0xf1] || head[..2] == [0xff, 0xf9] {
            Some(Self::Aac)
        } else {
            None
        }
    }

    #[must_use]
    pub fn is_same_as_ext(&self, name: impl AsRef<Path>) -> bool {
        if let Some(ext) = name.as_ref().extension()
            && let Some(ext) = ext.to_str()
        {
            ext.to_lowercase() == self.to_ext()
        } else {
            false
        }
    }

    #[must_use]
    const fn to_ext(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::Mp3 => "mp3",
            Self::M4a => "m4a",
            Self::Aac => "aac",
        }
    }

    const fn to_mime_str(self) -> &'static str {
        match self {
            Self::Wav => "audio/wav",
            Self::Flac => "audio/flac",
            Self::Ogg => "audio/ogg",
            Self::Mp3 => "audio/mpeg",
            Self::M4a => "audio/mp4",
            Self::Aac => "audio/aac",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    Deleted,
    Uploading,
    Pending,
    // used by (target_type, target_id)
    Used(Box<[(Stage, i64)]>),
}

impl Status {
    /// # Errors
    /// database error
    pub async fn get_by_id(
        trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        id: i64,
    ) -> anyhow::Result<Option<Self>> {
        let exists = is_exists_by_id(trans, id).await?;

        if !exists {
            return Ok(None);
        }

        let is_deleted = is_deleted_by_id(trans, id).await?;

        if is_deleted {
            return Ok(Some(Self::Deleted));
        }

        let is_pending = is_pending_by_id(trans, id).await?;

        if !is_pending {
            return Ok(Some(Self::Uploading));
        }

        let used = get_file_users_by_id(trans, id).await?;

        Ok(Some(
            if used.is_empty() {
                Self::Pending
            } else {
                Self::Used(used)
            },
        ))
    }
}

/// # Errors
/// database error
pub async fn is_exists_by_id(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> anyhow::Result<bool> {
    sqlx::query_scalar::<_, i64>(include_str!(
        "./sqls/is_file_exists_by_id.sql"
    ))
    .bind(id)
    .fetch_one(&mut **trans)
    .await
    .context("is_file_exists_by_id")
    .map(|it| it > 0)
}

/// # Errors
/// database error
pub async fn is_pending_by_id(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> anyhow::Result<bool> {
    sqlx::query_scalar::<_, i64>(include_str!(
        "./sqls/is_pending_by_file_id.sql"
    ))
    .bind(id)
    .fetch_one(&mut **trans)
    .await
    .context("is_pending_by_file_id")
    .map(|it| it > 0)
}

/// # Errors
/// database error
pub async fn is_deleted_by_id(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> anyhow::Result<bool> {
    sqlx::query_scalar::<_, i64>(include_str!(
        "./sqls/is_deleted_by_file_id.sql"
    ))
    .bind(id)
    .fetch_one(&mut **trans)
    .await
    .context("is_deleted_by_file_id")
    .map(|it| it > 0)
}

/// # Errors
/// database error
pub async fn get_file_users_by_id(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> anyhow::Result<Box<[(Stage, i64)]>> {
    let users = sqlx::query_as::<_, (String, i64)>(include_str!(
        "./sqls/get_file_users_by_id.sql"
    ))
    .bind(id)
    .fetch_all(&mut **trans)
    .await
    .context("get_file_users_by_id")?;

    let mut result = Vec::with_capacity(users.len());

    for (stage, target_id) in users {
        let stage: Stage =
            stage.parse().context("parse stage from db")?;
        result.push((stage, target_id));
    }

    Ok(result.into_boxed_slice())
}

/// check if the file is uploaded by the user
/// for the project and stage
///
/// or
///
/// check if the file is uploaded by the manager
/// for the user and project and stage
/// # Errors
/// database error
pub async fn is_uploaded_by(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    source: Source,
) -> anyhow::Result<bool> {
    sqlx::query_scalar::<_, i64>(include_str!(
        "./sqls/is_uploaded_by.sql"
    ))
    .bind(id)
    .bind(source.project_id)
    .bind(source.user_id)
    .bind(source.manager_id)
    .bind(source.manager_id)
    .bind(source.stage.into_str())
    .fetch_one(&mut **trans)
    .await
    .context("is_uploaded_by")
    .map(|it| it > 0)
}

/// expect valid `id`, and checked by `can_use_file`
/// # Errors
/// database error
pub async fn use_file(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    stage: Stage,
    target_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(include_str!("./sqls/ins_file_info.sql"))
        .bind(id)
        .bind(stage.into_str())
        .bind(target_id)
        .execute(&mut **trans)
        .await
        .context("ins_file_info")?;
    Ok(())
}

#[derive(Debug)]
pub struct DownloadInfo {
    pub name: Box<str>,
    pub s3_key: Box<str>,
}

/// # Errors
/// database error
pub async fn download_info(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    pid: i64,
) -> anyhow::Result<Option<DownloadInfo>> {
    let info = sqlx::query_as::<_, (String, String)>(include_str!(
        "./sqls/get_download_info.sql"
    ))
    .bind(id)
    .bind(pid)
    .fetch_optional(&mut **trans)
    .await
    .context("get_download_info")?
    .map(|(name, key)| DownloadInfo {
        name: name.into_boxed_str(),
        s3_key: key.into_boxed_str(),
    });

    Ok(info)
}

/// expect `group_id` valid
/// # Returns
/// distinct `file_id`s that is sorted in ascending order
/// # Errors
/// database error
pub async fn get_checked_files_by_group_id(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    group_id: i64,
) -> anyhow::Result<Vec<i64>> {
    sqlx::query_scalar(include_str!(
        "./sqls/get_checked_files_by_group_id.sql"
    ))
    .bind(group_id)
    .fetch_all(&mut **trans)
    .await
    .context("get_checked_files_by_group_id")
}

pub fn is_valid_filename(name: &str) -> bool {
    if !sanitize_filename::is_sanitized_with_options(
        name,
        sanitize_filename::OptionsForCheck {
            windows: true,
            truncate: false,
        },
    ) {
        tracing::debug!("file name contains invalid characters");
        return false;
    }

    if name.chars().count() > 128 {
        tracing::debug!("file name too long");
        return false;
    }

    true
}

/// # Errors
/// pre-sign failed
pub async fn pre_signed_get_simple(
    s3: &S3,
    key: impl Into<String>,
) -> anyhow::Result<PresignedReq> {
    s3.get_object()
        .bucket(s3.bucket())
        .key(key)
        .presigned(
            PresigningConfig::builder()
                .expires_in(Duration::from_secs(15 * 60))
                .build()
                .context("expect vaild expires_in")?,
        )
        .await
        .context("presigning download request")
        .map(Into::into)
}
