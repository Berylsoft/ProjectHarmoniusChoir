use anyhow::Context;
use axum::{extract::State, response::IntoResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::ManagerToken,
        shared::submit::{self, PreSubmitReviewRow},
        spawn_await,
    },
    api_assert, api_bail, api_bail_not_found, api_begin_transaction,
    api_param_assert,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PreSubmitReviewReq {
    pid: i64,
    sid: i64,
    status: submit::PreSubmitStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PreSubmitReviewRes {
    Success,
    AlreadyReviewed,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<PreSubmitReviewReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let response = spawn_await(do_review(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_review(
    db: Database,
    token: ManagerToken,
    req: PreSubmitReviewReq,
) -> ApiResult<PreSubmitReviewRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        token
            .verify_can_access_project(&mut trans, req.pid, false)
            .await?;

        let submit_pid_rid: Option<(Option<i64>, Option<i64>)> =
            sqlx::query_as(include_str!("./sqls/get_pid_rid_by_sid.sql"))
                .bind(req.sid)
                .fetch_optional(&mut *trans)
                .await
                .context("get_pid_rid_by_sid")?;
        let Some((submit_pid, submit_review_id)) = submit_pid_rid else {
            api_bail_not_found!("pre submit not found", "invalid sid")
        };

        if submit_review_id.is_some() {
            return Ok(PreSubmitReviewRes::AlreadyReviewed);
        }

        let Some(submit_pid) = submit_pid else {
            api_bail!(
                "expect valid project_user_id in status_pre_submits"
            );
        };
        api_param_assert!(submit_pid == req.pid, "bad pid/sid");

        let row = match req.status {
            submit::PreSubmitStatus::Rejected { reason } => {
                PreSubmitReviewRow {
                    status: submit::Status::Rejected.to_string().into(),
                    lead: None,
                    choir: None,
                    harmony: None,
                    reason: Some(reason.to_string().into()),
                }
            }
            submit::PreSubmitStatus::Passed(submit::GroupInfo {
                lead,
                choir,
                harmony,
            }) => PreSubmitReviewRow {
                status: submit::Status::Passed.to_string().into(),
                lead: Some(lead),
                choir: Some(choir),
                harmony: Some(harmony),
                reason: None,
            },
        };

        let ins_result =
            sqlx::query(include_str!("./sqls/ins_review_pre_submit.sql"))
                .bind(req.sid)
                .bind(token.mid)
                .bind(Utc::now().to_rfc3339())
                .bind(row.status)
                .bind(row.lead)
                .bind(row.choir)
                .bind(row.harmony)
                .bind(row.reason)
                .execute(&mut *trans)
                .await
                .context("ins_review_pre_submit")?;

        api_assert!(ins_result.rows_affected() == 1);

        ApiResult::Ok(PreSubmitReviewRes::Success)
    }
    .await;

    api::end_transaction(result, trans).await
}
