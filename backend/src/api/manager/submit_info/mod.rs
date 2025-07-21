use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::ManagerToken,
        shared::{
            file, project_user,
            submit::{SubmitReviewRow, SubmitStatus},
        },
        spawn_await,
    },
    api_bail_not_found, api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitInfoReq {
    puid: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitInfoRes {
    submits: Vec<SubmitInfo>,
    checked_files: Vec<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitInfo {
    id: i64,
    created_at: DateTime<Utc>,
    comment: Box<str>,
    files: Vec<file::Info>,
    status: Option<SubmitStatus>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<SubmitInfoReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let response =
        spawn_await(do_submit_info(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_submit_info(
    db: Database,
    token: ManagerToken,
    req: SubmitInfoReq,
) -> ApiResult<SubmitInfoRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        #[derive(Debug, FromRow)]
        struct SubmitInfoRow {
            id: i64,
            created_at: String,
            comment: String,
            r_status: Option<String>,
            r_reason: Option<String>,
            r_reason_detail: Option<String>,
            r_checked_file_group_id: Option<i64>,
        }

        token.verify(&mut trans).await?;

        let pid_uid =
            project_user::get_pid_uid_by_id(&mut trans, req.puid).await?;

        let Some((pid, _)) = pid_uid else {
            api_bail_not_found!(
                "project user not found",
                format!("can't find project user by puid: {}", req.puid)
            )
        };

        token.verify_can_access_project(&mut trans, pid).await?;

        let infos: Vec<SubmitInfoRow> = sqlx::query_as(include_str!(
            "./sqls/get_submit_info_by_puid.sql"
        ))
        .bind(req.puid)
        .fetch_all(&mut *trans)
        .await
        .context("get_submit_info_by_puid")?;

        let mut submits = Vec::with_capacity(infos.len());
        let mut checked_group: Option<i64> = None;

        for i in infos {
            let status = if let Some(status) = i.r_status {
                let status: SubmitStatus = SubmitReviewRow {
                    status,
                    reason: i.r_reason,
                    reason_detail: i.r_reason_detail,
                }
                .try_into()
                .context("SubmitReviewRow try_into PreSubmitStatus")?;

                Some(status)
            } else {
                None
            };

            let files =
                file::Info::get_all_of_submit_by_sid(&mut trans, i.id)
                    .await?;

            submits.push(SubmitInfo {
                id: i.id,
                created_at: i
                    .created_at
                    .parse()
                    .context("parse created_at into DateTime")?,
                comment: i.comment.into_boxed_str(),
                files,
                status,
            });

            if checked_group.is_none()
                && let Some(id) = i.r_checked_file_group_id
            {
                checked_group = Some(id);
            }
        }

        let checked_files = if let Some(group_id) = checked_group {
            file::get_checked_files_by_group_id(&mut trans, group_id)
                .await?
        } else {
            vec![]
        };

        ApiResult::Ok(SubmitInfoRes {
            submits,
            checked_files,
        })
    }
    .await;

    api::end_transaction(result, trans).await
}
