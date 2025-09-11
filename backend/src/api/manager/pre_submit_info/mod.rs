use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::{self, ManagerToken},
        shared::{
            file, project_user,
            submit::{self, PreSubmitReviewRow, PreSubmitStatus},
        },
        spawn_await,
    },
    api_bail_not_found, api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PreSubmitInfoReq {
    puid: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreSubmitInfoRes {
    pre_submits: Vec<PreSubmitInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PreSubmitInfo {
    id: i64,
    created_at: DateTime<Utc>,
    name: Box<str>,
    harmony_group_intention: Option<bool>,
    comment: Box<str>,
    file_info: Option<file::Info>,
    status: Option<submit::Detail<PreSubmitStatus>>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<PreSubmitInfoReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    // NOTE: api_param_assert
    let response =
        spawn_await(do_pre_submit_info(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_pre_submit_info(
    db: Database,
    token: ManagerToken,
    req: PreSubmitInfoReq,
) -> ApiResult<PreSubmitInfoRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        #[derive(Debug, FromRow)]
        struct PreSubmitInfoRow {
            id: i64,
            created_at: Box<str>,
            name: Box<str>,
            harmony_group_intention: Option<bool>,
            comment: Box<str>,
            f_id: Option<i64>,
            f_name: Option<Box<str>>,
            r_manager_id: i64,
            r_status: Option<Box<str>>,
            r_lead: Option<bool>,
            r_choir: Option<bool>,
            r_harmony: Option<bool>,
            r_choir_harmony: Option<bool>,
            r_reason: Option<Box<str>>,
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

        token
            .verify_can_access_project(&mut trans, pid, true)
            .await?;

        let infos: Vec<PreSubmitInfoRow> = sqlx::query_as(include_str!(
            "./sqls/get_pre_submit_info_by_puid.sql"
        ))
        .bind(req.puid)
        .fetch_all(&mut *trans)
        .await
        .context("get_pre_submit_info_by_puid")?;

        let mut pre_submits = Vec::with_capacity(infos.len());

        for i in infos {
            let status = if let Some(r_status) = i.r_status {
                let status: PreSubmitStatus = PreSubmitReviewRow {
                    status: r_status,
                    lead: i.r_lead,
                    choir: i.r_choir,
                    harmony: i.r_harmony,
                    choir_harmony: i.r_choir_harmony,
                    reason: i.r_reason,
                }
                .try_into()
                .context("PreSubmitReviewRow try_into PreSubmitStatus")?;

                let mname =
                    manager::get_name_by_id(&mut trans, i.r_manager_id)
                        .await?;

                Some(submit::Detail {
                    mid: i.r_manager_id,
                    mname,
                    status,
                })
            } else {
                None
            };

            pre_submits.push(PreSubmitInfo {
                id: i.id,
                created_at: i
                    .created_at
                    .parse()
                    .context("info.created_at.parse()")?,
                name: i.name,
                harmony_group_intention: i.harmony_group_intention,
                comment: i.comment,
                file_info: file::Info::try_from_pre_submit_row_optional(
                    (i.f_id, i.f_name),
                )
                .context(
                    "file::Info::try_from_pre_submit_row_optional",
                )?,
                status,
            });
        }

        ApiResult::Ok(PreSubmitInfoRes { pre_submits })
    }
    .await;

    api::end_transaction(result, trans).await
}
