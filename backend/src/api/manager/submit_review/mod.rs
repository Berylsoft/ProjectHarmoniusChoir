use std::collections::BTreeSet;

use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use chrono::Utc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::ManagerToken,
        shared::{
            file,
            submit::{self, SubmitReviewRow},
        },
        spawn_await,
    },
    api_assert, api_bail_not_found, api_bail_status,
    api_begin_transaction, api_param_assert,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitReviewReq {
    pid: i64,
    sid: i64,
    status: submit::SubmitStatus,
    checked_files: Option<Vec<i64>>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<SubmitReviewReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    spawn_await(do_submit_review(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(())))
}

#[expect(clippy::too_many_lines)]
async fn do_submit_review(
    db: Database,
    token: ManagerToken,
    req: SubmitReviewReq,
) -> ApiResult<(), ToCbor> {
    let checked_files = req.checked_files.map(|files| {
        let files = files.into_iter().collect::<BTreeSet<_>>();
        files.into_iter().collect_vec()
    });

    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        token
            .verify_can_access_project(&mut trans, req.pid, false)
            .await?;

        // broken invariant if puid or pid is null
        let puid_pid_rid: Option<(i64, i64, Option<i64>)> =
            sqlx::query_as(include_str!(
                "./sqls/get_puid_pid_rid_by_sid.sql"
            ))
            .bind(req.sid)
            .fetch_optional(&mut *trans)
            .await
            .context("get_puid_pid_rid_by_sid")?;
        let Some((p_uid, pid, rid)) = puid_pid_rid else {
            api_bail_not_found!(
                "submit not found",
                format!("invalid sid: {}", req.sid)
            )
        };

        if let Some(rid) = rid {
            api_bail_status!(
                "already reviewed",
                format!("s{} already reviewed by r{rid}", req.sid)
            );
        }

        api_param_assert!(req.pid == pid, "bad pid/sid");

        let mut group_id: Option<i64> = None;

        if let Some(files) = checked_files {
            let prev_group_id: Option<i64> =
                sqlx::query_scalar(include_str!(
                    "./sqls/get_prev_checked_files_group_id_by_puid.sql"
                ))
                .bind(p_uid)
                .fetch_one(&mut *trans)
                .await
                .context("get_prev_checked_files_group_id_by_puid")?;

            let need_update = if let Some(prev_group_id) = prev_group_id {
                let prev_files = file::get_checked_files_by_group_id(
                    &mut trans,
                    prev_group_id,
                )
                .await?;
                prev_files != files
            } else {
                true
            };

            if need_update {
                let latest_group_id: i64 =
                    sqlx::query_scalar(include_str!(
                        "./sqls/get_latest_checked_files_group_id.sql"
                    ))
                    .fetch_one(&mut *trans)
                    .await
                    .context("get_latest_checked_files_group_id")?;

                let new_group_id = latest_group_id + 1;
                group_id = Some(new_group_id);

                for file_id in files {
                    let ins_result = sqlx::query(include_str!(
                        "./sqls/ins_checked_file.sql"
                    ))
                    .bind(new_group_id)
                    .bind(file_id)
                    .execute(&mut *trans)
                    .await
                    .context("ins_checked_file")?;

                    api_assert!(ins_result.rows_affected() == 1);
                }
            }
        }

        let row = match req.status {
            submit::SubmitStatus::Rejected { reason, detail } => {
                SubmitReviewRow {
                    status: submit::Status::Rejected.to_string().into(),
                    reason: Some(reason.to_string().into()),
                    reason_detail: detail.map(|it| it.to_string().into()),
                }
            }
            submit::SubmitStatus::Passed => SubmitReviewRow {
                status: submit::Status::Passed.to_string().into(),
                reason: None,
                reason_detail: None,
            },
        };

        let ins_result =
            sqlx::query(include_str!("./sqls/ins_review_submit.sql"))
                .bind(req.sid)
                .bind(token.mid)
                .bind(Utc::now().to_rfc3339())
                .bind(group_id)
                .bind(row.status)
                .bind(row.reason)
                .bind(row.reason_detail)
                .execute(&mut *trans)
                .await
                .context("ins_review_submit")?;

        api_assert!(ins_result.rows_affected() == 1);

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}
