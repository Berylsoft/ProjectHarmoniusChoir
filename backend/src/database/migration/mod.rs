use anyhow::Context;
use itertools::Itertools;
use sqlx::Connection;

use super::{BeginStmt, try_end_transaction};

#[derive(Debug)]
pub struct Migration {
    version: i64,
    sql: &'static str,
}

impl Migration {
    pub const fn all() -> &'static [Self] {
        // NOTE: database with empty applied migration is considered version 0
        const ALL: &[Migration] = &[
            Migration {
                version: 1,
                sql: include_str!("./migrations/20250606_0001_init.sql"),
            },
            Migration {
                version: 2,
                sql: include_str!("./migrations/20250611_0001_views.sql"),
            },
            Migration {
                version: 3,
                sql: include_str!(
                    "./migrations/20250611_0002_indexes.sql"
                ),
            },
        ];

        let mut last_ver = 0;
        let mut idx = 0;
        while idx < ALL.len() {
            let it = &ALL[idx];
            assert!(last_ver < it.version);
            last_ver = it.version;
            idx += 1;
        }

        ALL
    }

    pub async fn run(pool: &sqlx::AnyPool) -> anyhow::Result<()> {
        let mut conn = pool
            .acquire()
            .await
            .context("failed to acquire connection")?;

        let mut trans = conn
            .begin_with(BeginStmt::Exclusive)
            .await
            .context("failed to begin transaction")?;

        let mut run_migrations = async || {
            sqlx::query(include_str!("./sqls/init.sql"))
                .execute(&mut *trans)
                .await
                .context("failed to init migrations")?;

            let version = sqlx::query_scalar::<_, i64>(include_str!(
                "./sqls/get_version.sql"
            ))
            .fetch_optional(&mut *trans)
            .await
            .context("failed to get current database version")?
            .unwrap_or(0);

            let migrations = Self::all()
                .iter()
                .sorted_by(|a, b| a.version.cmp(&b.version))
                .collect_vec();

            for migration in migrations {
                if migration.version <= version {
                    continue;
                }

                tracing::info!(
                    version = migration.version,
                    "running migration"
                );

                sqlx::raw_sql(migration.sql)
                    .execute(&mut *trans)
                    .await
                    .with_context(|| {
                    format!("failed to execute migration: {migration:?}")
                })?;

                sqlx::query(include_str!("./sqls/insert_applied.sql"))
                    .bind(migration.version)
                    .execute(&mut *trans)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to execute migration: {migration:?}"
                        )
                    })?;
            }

            anyhow::Result::<()>::Ok(())
        };

        try_end_transaction(run_migrations().await, trans)
            .await
            .context("failed to end transaction")?;

        Ok(())
    }
}
