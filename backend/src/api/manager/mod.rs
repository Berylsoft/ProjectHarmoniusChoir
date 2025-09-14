use std::sync::LazyLock;

use anyhow::{Context, ensure};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHashString, SaltString},
};
use chrono::{DateTime, Utc};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use sqlx::Transaction;
use tokio::sync::Semaphore;
use totp_rs::TOTP;

use crate::{
    ServerState,
    api::{ApiError, ApiResult, shared::project},
    api_begin_transaction,
    database::try_end_transaction,
    wechat::{Message, Wechat},
};

pub mod acquire_sudo;
pub mod bundle_job;
pub mod create_manager;
pub mod create_project;
pub mod delete_file;
pub mod gen_login_as;
pub mod get_file;
pub mod get_info;
pub mod list_managers;
pub mod list_pending_files;
pub mod list_project_users;
pub mod list_projects;
pub mod login;
pub mod master;
pub mod master_info;
pub mod pre_submit_info;
pub mod pre_submit_review;
pub mod project_manager_edit;
pub mod submit_info;
pub mod submit_review;
pub mod template;
pub mod upload_file;

const ROOT_MID: i64 = 0;
const DEFAULT_PASSWORD_LEN: usize = 32;
// TODO: actual parameter for production server
static ARGON2: LazyLock<argon2::Argon2> = LazyLock::new(|| {
    let params = argon2::Params::new(2 * 1024, 2, 1, Some(64)).unwrap();
    Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    )
});
static ARGON2_PARALLEL_SEMAPHORE: Semaphore = Semaphore::const_new(8);

#[derive(Debug, Serialize, Deserialize)]
pub struct ManagerToken {
    pub mid: i64,
    pub token_id: i64,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expired: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub sudo_expired: DateTime<Utc>,
}

impl ManagerToken {
    async fn verify<S>(
        &self,
        trans: &mut Transaction<'_, sqlx::Sqlite>,
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

    const fn is_root(&self) -> bool {
        self.mid == ROOT_MID
    }

    async fn verify_root<S>(
        &self,
        trans: &mut Transaction<'_, sqlx::Sqlite>,
    ) -> ApiResult<(), S> {
        self.verify(trans).await?;

        if !self.is_root() {
            return Err(ApiError::InsufficientPermission("require root"));
        }

        Ok(())
    }

    async fn verify_sudo<S>(
        &self,
        trans: &mut Transaction<'_, sqlx::Sqlite>,
        require_root: bool,
    ) -> ApiResult<(), S> {
        if require_root {
            self.verify_root(trans).await?;
        } else {
            self.verify(trans).await?;
        }

        if self.sudo_expired < Utc::now() {
            return Err(ApiError::RequireSudo);
        }

        Ok(())
    }

    async fn verify_totp(
        &self,
        trans: &mut Transaction<'_, sqlx::Sqlite>,
        totp_code: u32,
    ) -> anyhow::Result<bool> {
        verify_totp(trans, self.mid, totp_code).await
    }

    async fn verify_can_access_project<S>(
        &self,
        trans: &mut Transaction<'_, sqlx::Sqlite>,
        pid: i64,
        read_only_to_project: bool,
    ) -> ApiResult<(), S> {
        if self.is_root() {
            if !read_only_to_project {
                project::verify_not_ended(trans, pid).await?;
            }

            return Ok(());
        }

        let project_manager_id = sqlx::query_scalar::<_, i64>(
            include_str!("./sqls/get_project_manager_id_by_pid_mid.sql"),
        )
        .bind(pid)
        .bind(self.mid)
        .fetch_optional(&mut **trans)
        .await
        .context("get_project_manager_id_by_pid_mid")?;

        if project_manager_id.is_none() {
            return Err(ApiError::InsufficientPermission(
                "not a manager of this project",
            ));
        }

        if !read_only_to_project {
            project::verify_not_ended(trans, pid).await?;
        }

        Ok(())
    }
}

/// # Return
/// plain text password if initialized
/// # Errors
/// database error
/// # Panics
/// broken hash
pub async fn init_root_if_not_exists(
    state: &ServerState,
) -> anyhow::Result<Option<Box<str>>> {
    let (password, password_hash) = generate_default_password().await?;

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
}

/// # Returns
/// true if valid password
async fn verify_password<S>(
    pswd_sha512: [u8; 64],
    stored_password: &PasswordHash<'_>,
) -> ApiResult<bool, S> {
    let permit = ARGON2_PARALLEL_SEMAPHORE
        .acquire()
        .await
        .expect("not closed");

    let stored_password = stored_password.serialize();
    let verify_res = tokio::task::spawn_blocking(move || {
        let result = ARGON2.verify_password(
            &pswd_sha512,
            &stored_password.password_hash(),
        );

        drop(permit);

        result
    })
    .await
    .context("failed to wait password verify to return")?;

    if verify_res == Err(argon2::password_hash::Error::Password) {
        return Ok(false);
    }

    verify_res.context("verify password")?;
    Ok(true)
}

async fn hash_password(
    pswd_sha512: [u8; 64],
) -> anyhow::Result<PasswordHashString> {
    let permit = ARGON2_PARALLEL_SEMAPHORE
        .acquire()
        .await
        .expect("not closed");

    let password_hash = tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut rand_core::OsRng);
        let password_hash = ARGON2
            .hash_password(&pswd_sha512, &salt)
            .context("Argon2::hash_password")?;
        let password_hash = password_hash.serialize();

        drop(permit);

        anyhow::Result::<_>::Ok(password_hash)
    })
    .await
    .context("failed to wait password verify to return")??;

    Ok(password_hash)
}

async fn generate_default_password()
-> anyhow::Result<(Box<str>, PasswordHashString)> {
    let mut password = String::with_capacity(DEFAULT_PASSWORD_LEN);
    Alphanumeric.append_string(
        &mut rand::rng(),
        &mut password,
        DEFAULT_PASSWORD_LEN,
    );
    let password_hash = Sha512::digest(password.as_bytes());
    let password_hash = hash_password(password_hash.into())
        .await
        .context("hash_password")?;

    Ok((password.into(), password_hash))
}

// TODO: update password hash when parameter changed

fn totp_new(secret: Vec<u8>, mid: impl Into<Option<i64>>) -> TOTP {
    let mid: Option<i64> = mid.into();
    TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        secret,
        Some("LuminizorsPassportForAdmins".to_string()),
        mid.map(|it| it.to_string()).unwrap_or_default(),
    )
    .unwrap()
}

/// # Returns
/// true if valid `totp_code`
fn totp_check(secret: Vec<u8>, totp_code: u32) -> anyhow::Result<bool> {
    totp_new(secret, None)
        .check_current(&format!("{totp_code:0>6}"))
        .context("failed to get system time")
}

/// expect mid exists
async fn verify_totp(
    trans: &mut Transaction<'_, sqlx::Sqlite>,
    mid: i64,
    totp_code: u32,
) -> anyhow::Result<bool> {
    let totp_secret = sqlx::query_scalar::<_, Vec<u8>>(include_str!(
        "./sqls/get_manager_totp_secret_by_mid.sql"
    ))
    .bind(mid)
    .fetch_one(&mut **trans)
    .await
    .context("get_manager_totp_secret_by_mid")?;

    totp_check(totp_secret, totp_code)
}

/// expect manager exists
async fn get_name_by_id<S>(
    trans: &mut Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> ApiResult<Box<str>, S> {
    let name =
        sqlx::query_scalar(include_str!("./sqls/get_name_by_id.sql"))
            .bind(id)
            .fetch_one(&mut **trans)
            .await
            .context("get_name_by_id")?;
    Ok(name)
}

async fn send_message_infallible(
    trans: &mut Transaction<'_, sqlx::Sqlite>,
    wechat: &dyn Wechat,
    project_id: i64,
    uid: i64,
    message: Message,
) -> anyhow::Result<()> {
    // broken invariant if user not exists
    let openid: Option<Box<str>> =
        sqlx::query_scalar(include_str!("./sqls/get_openid_by_uid.sql"))
            .bind(uid)
            .fetch_one(&mut **trans)
            .await
            .context("get_openid_by_uid")?;

    let Some(openid) = openid else {
        return Ok(());
    };

    let page = format!("/recruit/{project_id}").into();

    wechat.send_message_infallible(page, openid, message).await;

    Ok(())
}
