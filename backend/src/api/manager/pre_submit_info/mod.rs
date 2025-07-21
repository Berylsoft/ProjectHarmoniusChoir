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
            submit::{PreSubmitReviewRow, PreSubmitStatus},
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
    harmony_group_intention: Option<bool>,
    comment: Box<str>,
    file_info: file::Info,
    status: Option<PreSubmitStatus>,
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
            created_at: String,
            harmony_group_intention: Option<bool>,
            comment: String,
            f_id: i64,
            f_name: String,
            r_status: Option<String>,
            r_lead: Option<bool>,
            r_choir: Option<bool>,
            r_harmony: Option<bool>,
            r_reason: Option<String>,
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
                    reason: i.r_reason,
                }
                .try_into()
                .context("PreSubmitReviewRow try_into PreSubmitStatus")?;

                Some(status)
            } else {
                None
            };

            pre_submits.push(PreSubmitInfo {
                id: i.id,
                created_at: i
                    .created_at
                    .parse()
                    .context("info.created_at.parse()")?,
                harmony_group_intention: i.harmony_group_intention,
                comment: i.comment.into_boxed_str(),
                file_info: file::Info {
                    id: i.f_id,
                    name: i.f_name.into_boxed_str(),
                },
                status,
            });
        }

        ApiResult::Ok(PreSubmitInfoRes { pre_submits })
    }
    .await;

    api::end_transaction(result, trans).await
}
