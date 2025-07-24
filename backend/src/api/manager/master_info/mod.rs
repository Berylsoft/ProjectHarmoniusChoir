use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor, manager::ManagerToken,
        shared::project_user, spawn_await,
    },
    api_bail_not_found, api_bail_status, api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct MasterInfoReq {
    puid: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MasterInfoRes {
    mid: i64,
    created_at: DateTime<Utc>,
    comment: Box<str>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<MasterInfoReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let response =
        spawn_await(do_master_info(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_master_info(
    db: Database,
    token: ManagerToken,
    req: MasterInfoReq,
) -> ApiResult<MasterInfoRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        #[derive(Debug, FromRow)]
        struct MasterInfoRow {
            manager_id: i64,
            created_at: String,
            comment: String,
        }

        token.verify(&mut trans).await?;

        let pid_uid =
            project_user::get_pid_uid_by_id(&mut trans, req.puid).await?;

        let Some((pid, _)) = pid_uid else {
            // ensure by client
            api_bail_not_found!(
                "project user not found",
                format!("can't find project user by puid: {}", req.puid)
            )
        };

        token
            .verify_can_access_project(&mut trans, pid, true)
            .await?;

        let info: Option<MasterInfoRow> = sqlx::query_as(include_str!(
            "./sqls/get_master_info_by_puid.sql"
        ))
        .bind(req.puid)
        .fetch_optional(&mut *trans)
        .await
        .context("get_master_info_by_puid")?;

        let Some(info) = info else {
            api_bail_status!(
                "not mastered yet",
                format!("project user {} is not mastered", req.puid)
            );
        };

        ApiResult::Ok(MasterInfoRes {
            mid: info.manager_id,
            created_at: info
                .created_at
                .parse()
                .context("parse created_at into DateTime")?,
            comment: info.comment.into_boxed_str(),
        })
    }
    .await;

    api::end_transaction(result, trans).await
}
