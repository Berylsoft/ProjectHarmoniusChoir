use std::cmp;

use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToJson,
        shared::project_user::{self, NdaStatus},
        user::UserToken,
    },
    api_assert, api_bail, api_bail_status, api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct AgreeNdaReq {
    pid: i64,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<AgreeNdaReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    api::spawn_await(do_agree_nda(db, token.0, req)).await??;

    Ok(Json(api::Response::Ok(())))
}

async fn do_agree_nda(
    db: Database,
    token: UserToken,
    req: AgreeNdaReq,
) -> ApiResult<(), ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        let puid =
            token.verify_joined_project(&mut trans, req.pid).await?;

        let nda_status =
            NdaStatus::get_by_pid_puid(&mut trans, req.pid, puid)
                .await
                .context("NdaStatus::get_by_pid_puid")?;

        if !matches!(nda_status, project_user::NdaStatus::Pending(_)) {
            api_bail_status!(
                "invalid status",
                format!("invalid nda status: {nda_status:?}")
            );
        }

        let (status, _) =
            project_user::Status::get_by_puid(&mut trans, puid)
                .await
                .context("project_user::Status::get_by_puid")?;

        match status.cmp(&project_user::Status::PreSubmitPassed) {
            cmp::Ordering::Less => api_bail_status!(
                "invalid status",
                format!("invalid nda status: {nda_status:?}")
            ),
            cmp::Ordering::Greater => api_bail!(
                "unexpected pending nda after pre-submit passed"
            ),
            cmp::Ordering::Equal => {
                let ins_result = sqlx::query(include_str!(
                    "./sqls/ins_nda_agreed.sql"
                ))
                .bind(puid)
                .execute(&mut *trans)
                .await
                .context("ins_nda_agreed")?;

                api_assert!(ins_result.rows_affected() == 1);
            }
        }

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}
