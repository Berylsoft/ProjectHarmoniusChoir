use std::{
    borrow::Cow,
    ops::{Deref, DerefMut},
};

use anyhow::{Context, Result};
use migration::Migration;
use sqlx::{
    Database as SqlxDatabase, Transaction, any::install_default_drivers,
};

pub mod migration;

#[derive(Debug, Clone)]
pub struct Database {
    pool: sqlx::AnyPool,
}

impl Database {
    /// # Errors
    /// when database calls failed
    #[tracing::instrument(fields(url = url.as_ref()))]
    pub async fn init(url: impl AsRef<str>) -> anyhow::Result<Self> {
        tracing::info!("initialzing database");
        install_default_drivers();

        let pool = sqlx::any::AnyPoolOptions::new()
            .connect(url.as_ref())
            .await
            .context("failed to connect")?;

        Migration::run(&pool)
            .await
            .context("failed to run migrations")?;

        Ok(Self { pool })
    }
}

impl Deref for Database {
    type Target = sqlx::AnyPool;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl DerefMut for Database {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pool
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum BeginStmt {
    Deferred,
    Immediate,
    #[default]
    Exclusive,
}

impl BeginStmt {
    #[must_use]
    pub const fn stmt(self) -> &'static str {
        match self {
            Self::Deferred => "BEGIN DEFERRED",
            Self::Immediate => "BEGIN IMMEDIATE",
            Self::Exclusive => "BEGIN EXCLUSIVE",
        }
    }
}

impl From<BeginStmt> for Cow<'static, str> {
    fn from(value: BeginStmt) -> Self {
        value.stmt().into()
    }
}

// pub async fn transaction_with<'a, C, Fut, F, R, E>(
//     conn: &'a mut C,
//     begin: BeginTransaction,
//     callback: F,
// ) -> Result<R, E>
// where
//     Fut: Future<Output = Result<R, E>> + 'a,
//     for<'c> F: FnOnce(&'c mut Transaction<'_, C::Database>) -> Fut
//         + 'a
//         + Send
//         + Sync,
//     C: Connection + Sized,
//     R: Send,
//     E: From<sqlx::Error> + Send,
// {
//     let mut transaction = conn.begin_with(begin.stmt()).await?;
//     let ret = callback(&mut transaction).await;
//
//     try_end_transaction(ret, transaction).await
// }

/// # Errors
/// When failed to commit or rollback
pub async fn try_end_transaction<DB, R, E>(
    result: Result<R, E>,
    trans: Transaction<'_, DB>,
) -> Result<Result<R, E>, sqlx::Error>
where
    DB: SqlxDatabase,
{
    if result.is_ok() {
        trans.commit().await?;
    } else {
        trans.rollback().await?;
    }

    Ok(result)
}

/// # Errors
/// database errors
pub async fn last_insert_rowid(
    trans: &mut sqlx::Transaction<'_, sqlx::Any>,
) -> anyhow::Result<i64> {
    sqlx::query_scalar::<_, i64>(include_str!(
        "./sqls/get_last_insert_rowid.sql"
    ))
    .fetch_one(&mut **trans)
    .await
    .context("get_last_insert_rowid")
}
