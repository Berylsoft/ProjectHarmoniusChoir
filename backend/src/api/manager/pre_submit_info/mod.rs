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
            file,
            project_user::get_project_user_id_by_pid_uid,
            submit::{self, PreSubmitStatus},
        },
        spawn_await,
    },
    api_bail, api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PreSubmitInfoReq {
    pid: i64,
    uid: i64,
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
        token.verify_can_access_project(&mut trans, req.pid).await?;

        let project_uid =
            get_project_user_id_by_pid_uid(&mut trans, req.uid, req.pid)
                .await?;

        let Some(project_uid) = project_uid else {
            return Err(ApiError::BadParam {
                msg: "invalid uid/pid".into(),
                detail: format!(
                    "can't found project user by uid({}), pid({})",
                    req.uid, req.pid
                )
                .into_boxed_str(),
            });
        };

        let infos: Vec<PreSubmitInfoRow> = sqlx::query_as(include_str!(
            "./sqls/get_pre_submit_info_by_puid.sql"
        ))
        .bind(project_uid)
        .fetch_all(&mut *trans)
        .await
        .context("get_pre_submit_info_by_puid")?;

        let mut pre_submits = Vec::with_capacity(infos.capacity());

        for i in infos {
            let status = if let Some(r_status) = i.r_status {
                let status: submit::Status = r_status
                    .parse()
                    .context("parse submit::Status from db result")?;

                Some(match status {
                    submit::Status::Rejected => {
                        PreSubmitStatus::Rejected {
                            reason: i
                                .r_reason
                                .context("get reason when rejected")?
                                .parse()
                                .context("parse reason from db result")?,
                        }
                    }
                    submit::Status::Passed => {
                        let (Some(lead), Some(choir), Some(harmony)) =
                            (i.r_lead, i.r_choir, i.r_harmony)
                        else {
                            api_bail!("get group info when passed");
                        };
                        PreSubmitStatus::Passed {
                            lead,
                            choir,
                            harmony,
                        }
                    }
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
