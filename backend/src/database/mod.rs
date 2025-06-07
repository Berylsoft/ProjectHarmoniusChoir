use std::{
    borrow::Cow,
    ops::{Deref, DerefMut},
};

use anyhow::{Context, Result};
use migration::Migration;
use sqlx::{
    Database as SqlxDatabase, Transaction, any::install_default_drivers,
    sqlite::SqliteConnectOptions,
};
use tracing::{debug, instrument};

pub mod migration;

#[derive(Clone)]
pub struct Database {
    pool: sqlx::AnyPool,
}

impl Database {
    #[instrument(level = "debug", skip(url))]
    pub async fn init(url: &str) -> anyhow::Result<Self> {
        debug!("initialzing database with url: {url}");
        install_default_drivers();

        let pool = sqlx::any::AnyPoolOptions::new()
            .connect(url)
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

pub async fn try_end_transaction<DB, R, E>(
    result: Result<R, E>,
    trans: Transaction<'_, DB>,
) -> Result<R, E>
where
    E: From<sqlx::Error>,
    DB: SqlxDatabase,
{
    match result {
        Ok(ret) => {
            trans.commit().await?;

            Ok(ret)
        }
        Err(err) => {
            trans.rollback().await?;

            Err(err)
        }
    }
}
