use anyhow::Context;
use axum::{extract::State, response::IntoResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    ArcWechat, ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::{ManagerToken, send_message_infallible},
        shared::submit::{self, PreSubmitReviewRow},
        spawn_await,
    },
    api_assert, api_bail_not_found, api_bail_status,
    api_begin_transaction, api_param_assert,
    database::Database,
    extractors::{Cbor, Token},
    wechat::{self, Message},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PreSubmitReviewReq {
    pid: i64,
    sid: i64,
    status: submit::PreSubmitStatus,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<PreSubmitReviewReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState {
        mut cache,
        db,
        wechat,
        ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    spawn_await(do_review(db, wechat, token.0, req)).await??;
    Ok(Cbor(api::Response::Ok(())))
}

async fn do_review(
    db: Database,
    wechat: ArcWechat,
    token: ManagerToken,
    req: PreSubmitReviewReq,
) -> ApiResult<(), ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        token
            .verify_can_access_project(&mut trans, req.pid, false)
            .await?;

        // broken invariant if pid or uid is null
        let submit_pid_uid_rid: Option<(i64, i64, Option<i64>)> =
            sqlx::query_as(include_str!("./sqls/get_pid_rid_by_sid.sql"))
                .bind(req.sid)
                .fetch_optional(&mut *trans)
                .await
                .context("get_pid_rid_by_sid")?;
        let Some((submit_pid, uid, submit_review_id)) =
            submit_pid_uid_rid
        else {
            api_bail_not_found!("pre submit not found", "invalid sid")
        };

        if submit_review_id.is_some() {
            api_bail_status!(
                "already reviewed",
                format!("{} already reviewed", req.sid)
            );
        }

        api_param_assert!(submit_pid == req.pid, "bad pid/sid");

        let message_result = match &req.status {
            submit::PreSubmitStatus::Rejected { .. } => {
                wechat::ReviewResult::Rejected
            }
            submit::PreSubmitStatus::Passed(_) => {
                wechat::ReviewResult::Passed
            }
        };

        let row = match req.status {
            submit::PreSubmitStatus::Rejected { reason } => {
                PreSubmitReviewRow {
                    status: submit::Status::Rejected.to_string().into(),
                    lead: None,
                    choir: None,
                    harmony: None,
                    choir_harmony: None,
                    reason: Some(reason.to_string().into()),
                }
            }
            submit::PreSubmitStatus::Passed(submit::GroupInfo {
                lead,
                choir,
                harmony,
                choir_harmony,
            }) => PreSubmitReviewRow {
                status: submit::Status::Passed.to_string().into(),
                lead: Some(lead),
                choir: Some(choir),
                harmony: Some(harmony),
                choir_harmony: Some(choir_harmony),
                reason: None,
            },
        };

        let now = Utc::now();

        let ins_result =
            sqlx::query(include_str!("./sqls/ins_review_pre_submit.sql"))
                .bind(req.sid)
                .bind(token.mid)
                .bind(now.to_rfc3339())
                .bind(row.status)
                .bind(row.lead)
                .bind(row.choir)
                .bind(row.harmony)
                .bind(row.choir_harmony)
                .bind(row.reason)
                .execute(&mut *trans)
                .await
                .context("ins_review_pre_submit")?;

        api_assert!(ins_result.rows_affected() == 1);

        let message = Message {
            content: wechat::ReviewContent::PreSubmit,
            result: message_result,
            time: now,
        };

        send_message_infallible(
            &mut trans,
            wechat.as_ref(),
            submit_pid,
            uid,
            message,
        )
        .await
        .context("send_message_infallible")?;

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}
