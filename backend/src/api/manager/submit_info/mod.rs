use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
    ServerState,
    api::{
        self, ApiError, ApiResult, ToCbor,
        manager::ManagerToken,
        shared::{
            file, project_user,
            submit::{SubmitReviewRow, SubmitStatus},
        },
        spawn_await,
    },
    api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitInfoReq {
    pid: i64,
    uid: i64,
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
        token.verify_can_access_project(&mut trans, req.pid).await?;

        // pid verified verify_can_access_project
        let puid =
            project_user::get_id_by_pid_uid(&mut trans, req.uid, req.pid)
                .await?;

        let Some(puid) = puid else {
            return Err(ApiError::BadParam {
                msg: "invalid uid/pid".into(),
                detail: format!(
                    "can't found project user by uid({}), pid({})",
                    req.uid, req.pid
                )
                .into_boxed_str(),
            });
        };

        let infos: Vec<SubmitInfoRow> = sqlx::query_as(include_str!(
            "./sqls/get_submit_info_by_puid.sql"
        ))
        .bind(puid)
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
            sqlx::query_scalar(include_str!(
                "./sqls/get_checked_files_by_group_id.sql"
            ))
            .bind(group_id)
            .fetch_all(&mut *trans)
            .await
            .context("get_checked_files_by_group_id")?
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
