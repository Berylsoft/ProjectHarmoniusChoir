use std::collections::HashSet;

use anyhow::Context as _;
use aws_sdk_s3::primitives::ByteStream;
use axum::{extract::State, response::IntoResponse};
use chrono::Utc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use tempfile::NamedTempFile;
use tokio::sync::oneshot;

use crate::{
    PendingJob, PendingJobs, S3, ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::ManagerToken,
        shared::{
            file::{self, PresignedReq},
            project_user,
        },
        spawn_await,
    },
    api_assert, api_bail_not_found, api_bail_status,
    api_begin_transaction, api_param_assert,
    database::{Database, try_end_transaction},
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub enum BundleJobReq {
    Submit { pid: i64, puids: Vec<i64> },
    List { pid: i64 },
    Download { job_id: i64 },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BundleJobRes {
    Success,
    Jobs { jobs: Vec<BundleJob> },
    Download { presigned_req: PresignedReq },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BundleJob {
    id: i64,
    finished: bool,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<BundleJobReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState {
        mut cache,
        db,
        s3,
        pending_jobs,
        ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let response = match req {
        BundleJobReq::Submit { pid, puids } => {
            spawn_await(do_job_submit(
                db,
                s3,
                pending_jobs,
                token.0,
                pid,
                puids,
            ))
            .await??
        }
        BundleJobReq::List { pid } => {
            spawn_await(do_job_list(db, token.0, pid)).await??
        }
        BundleJobReq::Download { job_id } => {
            spawn_await(do_job_download(db, s3, token.0, job_id))
                .await??
        }
    };

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_job_submit(
    db: Database,
    s3: S3,
    jobs: PendingJobs,
    token: ManagerToken,
    pid: i64,
    puids: Vec<i64>,
) -> ApiResult<BundleJobRes, ToCbor> {
    let distinct_len =
        puids.iter().copied().collect::<HashSet<_>>().len();
    api_param_assert!(
        puids.len() == distinct_len,
        "invalid project users"
    );

    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify_sudo(&mut trans, false).await?;
        token
            .verify_can_access_project(&mut trans, pid, false)
            .await?;

        for &puid in &puids {
            let pid_uid =
                project_user::get_pid_uid_by_id(&mut trans, puid).await?;

            let Some((..)) = pid_uid else {
                // ensure by client
                api_bail_not_found!(
                    "project user not found",
                    format!("can't find project user by puid: {}", puid)
                )
            };
        }

        let mut job_users: Vec<JobUser> = Vec::with_capacity(puids.len());

        for &puid in &puids {
            let submitted: i64 = sqlx::query_scalar(include_str!(
                "./sqls/is_submited_by_puid.sql"
            ))
            .bind(puid)
            .fetch_one(&mut *trans)
            .await
            .context("is_submited_by_puid")?;

            if submitted > 0 {
                api_bail_status!(
                    "invalid project user status",
                    format!("{puid} already submitted")
                )
            }

            let is_mastered: i64 = sqlx::query_scalar(include_str!(
                "./sqls/is_mastered_by_puid.sql"
            ))
            .bind(puid)
            .fetch_one(&mut *trans)
            .await
            .context("is_mastered_by_puid")?;

            if is_mastered == 0 {
                api_bail_status!(
                    "invalid project user status",
                    format!("{puid} is not mastered")
                );
            }

            // expect pre-submit passed when mastered
            let name =
                project_user::get_name_by_id(&mut trans, puid).await?;

            let files =
                get_master_files_by_puid(&mut trans, puid).await?;

            job_users.push(JobUser {
                id: puid,
                name,
                files,
            });
        }

        // ==================== write boundary ====================

        let job_id: i64 =
            sqlx::query_scalar(include_str!("./sqls/ins_job.sql"))
                .bind(pid)
                .bind(token.mid)
                .bind(Utc::now().to_rfc3339())
                .fetch_one(&mut *trans)
                .await
                .context("ins_job")?;

        for puid in puids {
            let ins_result =
                sqlx::query(include_str!("./sqls/ins_job_include.sql"))
                    .bind(job_id)
                    .bind(puid)
                    .execute(&mut *trans)
                    .await
                    .context("ins_job_include")?;

            api_assert!(ins_result.rows_affected() == 1);
        }

        spawn_job(
            jobs,
            db,
            s3,
            Job {
                id: job_id,
                pid,
                mid: token.mid,
                users: job_users,
            },
        );

        ApiResult::Ok(BundleJobRes::Success)
    }
    .await;

    api::end_transaction(result, trans).await
}

async fn do_job_list(
    db: Database,
    token: ManagerToken,
    pid: i64,
) -> ApiResult<BundleJobRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;
        token
            .verify_can_access_project(&mut trans, pid, true)
            .await?;

        let jobs: Vec<(i64, i64)> = sqlx::query_as(include_str!(
            "./sqls/get_all_jobs_by_pid.sql"
        ))
        .bind(pid)
        .fetch_all(&mut *trans)
        .await
        .context("get_all_jobs_by_pid")?;

        let jobs = jobs
            .into_iter()
            .map(|(id, finished)| BundleJob {
                id,
                finished: finished > 0,
            })
            .collect_vec();

        ApiResult::Ok(BundleJobRes::Jobs { jobs })
    }
    .await;

    api::end_transaction(result, trans).await
}

async fn do_job_download(
    db: Database,
    s3: S3,
    token: ManagerToken,
    job_id: i64,
) -> ApiResult<BundleJobRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;

        let job: Option<(i64, Option<String>)> =
            sqlx::query_as(include_str!("./sqls/get_job_info_by_id.sql"))
                .bind(job_id)
                .fetch_optional(&mut *trans)
                .await
                .context("get_job_info_by_id")?;

        let Some((pid, s3_key)) = job else {
            api_bail_not_found!("job not found");
        };

        token
            .verify_can_access_project(&mut trans, pid, true)
            .await?;

        let Some(s3_key) = s3_key else {
            api_bail_status!("job not finish");
        };

        let presigned_req =
            file::pre_signed_get_simple(&s3, s3_key).await?;

        ApiResult::Ok(BundleJobRes::Download { presigned_req })
    }
    .await;

    api::end_transaction(result, trans).await
}

struct Job {
    id: i64,
    pid: i64,
    mid: i64,
    users: Vec<JobUser>,
}

struct JobUser {
    id: i64,
    name: Box<str>,
    files: Vec<JobUserFile>,
}

#[derive(Debug, FromRow)]
struct JobUserFile {
    id: i64,
    name: Box<str>,
    s3_key: Box<str>,
}

fn spawn_job(pending_jobs: PendingJobs, db: Database, s3: S3, job: Job) {
    tokio::spawn(async move {
        let id = job.id;
        tracing::info!("starting bundle job {id}");
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let (wait_tx, wait_rx) = oneshot::channel();

        let mut jobs = pending_jobs.lock().await;
        jobs.insert(
            id,
            PendingJob {
                cancel: Some(cancel_tx),
                wait: Some(wait_rx),
            },
        );
        drop(jobs);

        let result = run_job(db, s3, cancel_rx, job).await;
        if let Err(err) = result {
            tracing::error!("job exited with error: {err}");
        } else {
            tracing::info!("bundle job {id} finished");
        }

        let mut jobs = pending_jobs.lock().await;
        jobs.remove(&id);
        let _ = wait_tx.send(());
        drop(jobs);
    });
}

#[expect(clippy::too_many_lines)]
#[tracing::instrument(skip(db, s3, cancel, job), fields(job.id))]
async fn run_job(
    db: Database,
    s3: S3,
    mut cancel: oneshot::Receiver<()>,
    job: Job,
) -> anyhow::Result<()> {
    macro_rules! check_cancel {
        ($cancel:expr) => {
            match $cancel.try_recv() {
                Ok(())
                | Err(
                    ::tokio::sync::oneshot::error::TryRecvError::Closed,
                ) => {
                    ::anyhow::bail!("canceled");
                }
                _ => {}
            }
        };
    }

    let temp_file = NamedTempFile::new().context("tempfile")?;
    let mut tar_builder = tar::Builder::new(temp_file);
    let base_path = job.id.to_string();

    for ju in &job.users {
        for juf in &ju.files {
            check_cancel!(cancel);

            tracing::debug!("donwloading {}", juf.s3_key);
            let file = s3
                .get_object()
                .bucket(s3.bucket())
                .key(&*juf.s3_key)
                .send()
                .await
                .with_context(|| format!("donwload {}", juf.s3_key))?;

            let body = file
                .body
                .collect()
                .await
                .context("body.collect")?
                .to_vec();

            let file_name =
                format!("{}_{}_{}_{}", ju.id, ju.name, juf.id, juf.name);
            // expect:
            // puid <= 20 digit
            // uname <= 20 character
            // file id <= 20 digit
            // file name <= 128
            // so 20 + 1 + 20 + 1 + 20 + 1 + 128 = 191
            // so no truncate is needed for gnu header
            let file_name = sanitize_filename::sanitize_with_options(
                file_name,
                sanitize_filename::Options {
                    windows: true,
                    truncate: false,
                    replacement: "_",
                },
            );

            let mut tar_header = tar::Header::new_gnu();
            tar_header.set_size(
                body.len().try_into().context("file size too big")?,
            );
            tar_header.set_mode(0o644);
            tar_header.set_entry_type(tar::EntryType::file());
            tar_builder
                .append_data(
                    &mut tar_header,
                    format!("{base_path}/{file_name}"),
                    body.as_slice(),
                )
                .context("tar_builder.append_data")?;
        }
    }

    let temp_file =
        tar_builder.into_inner().context("tar_builder.finish")?;

    check_cancel!(cancel);

    let key = format!("/uploads/{}/mixed/{}.tar", job.pid, job.id);
    let body = ByteStream::from_path(temp_file.path())
        .await
        .context("ByteStream::from_path(temp_file.path())")?;

    s3.put_object()
        .bucket(s3.bucket())
        .key(&key)
        .body(body)
        .content_type("application/x-tar")
        .send()
        .await
        .context("put bundled file")?;

    check_cancel!(cancel);

    api_begin_transaction!(db, conn, trans, Immediate);

    let result: anyhow::Result<()> = async {
        let time = Utc::now().to_rfc3339();
        for ju in job.users {
            let ins_result =
                sqlx::query(include_str!("./sqls/ins_mixed.sql"))
                    .bind(ju.id)
                    .bind(job.mid)
                    .bind(job.id)
                    .bind(&time)
                    .execute(&mut *trans)
                    .await
                    .context("ins_mixed")?;

            anyhow::ensure!(ins_result.rows_affected() == 1);
        }

        let ins_result =
            sqlx::query(include_str!("./sqls/ins_job_finished.sql"))
                .bind(job.id)
                .bind(time)
                .bind(key)
                .execute(&mut *trans)
                .await
                .context("ins_job_finished")?;

        anyhow::ensure!(ins_result.rows_affected() == 1);

        Ok(())
    }
    .await;

    try_end_transaction(result, trans)
        .await
        .context("end transaction")??;

    Ok(())
}

/// # Errors
/// database errors
pub async fn resume_jobs(state: ServerState) -> anyhow::Result<()> {
    #[derive(Debug, FromRow)]
    struct JobRow {
        id: i64,
        project_id: i64,
        manager_id: i64,
    }

    api_begin_transaction!(state.db, conn, trans, Deferred);

    let result: anyhow::Result<()> = async {
        let jobs: Vec<JobRow> =
            sqlx::query_as(include_str!("./sqls/get_pending_jobs.sql"))
                .fetch_all(&mut *trans)
                .await
                .context("get_pending_jobs")?;

        for job in jobs {
            let puids: Vec<i64> = sqlx::query_scalar(include_str!(
                "./sqls/get_included_puids_by_job_id.sql"
            ))
            .bind(job.id)
            .fetch_all(&mut *trans)
            .await
            .context("get_included_puids_by_job_id")?;

            let mut users = Vec::with_capacity(puids.len());

            for puid in puids {
                // expect pre-submit passed checked when submit job
                let name = project_user::get_name_by_id(&mut trans, puid)
                    .await?;

                let files =
                    get_master_files_by_puid(&mut trans, puid).await?;

                users.push(JobUser {
                    id: puid,
                    name,
                    files,
                });
            }

            let state = state.clone();
            spawn_job(
                state.pending_jobs,
                state.db,
                state.s3,
                Job {
                    id: job.id,
                    pid: job.project_id,
                    mid: job.manager_id,
                    users,
                },
            );
        }

        Ok(())
    }
    .await;

    try_end_transaction(result, trans)
        .await
        .context("end transaction")?
}

async fn get_master_files_by_puid(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    puid: i64,
) -> Result<Vec<JobUserFile>, anyhow::Error> {
    sqlx::query_as(include_str!("./sqls/get_master_files_by_puid.sql"))
        .bind(puid)
        .fetch_all(&mut **trans)
        .await
        .context("get_master_files_by_puid")
}
