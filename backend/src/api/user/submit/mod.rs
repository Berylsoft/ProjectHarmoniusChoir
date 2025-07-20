use std::collections::HashSet;

use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToJson,
        shared::{file, project_user},
        user::UserToken,
    },
    api_bail_not_found, api_bail_status, api_begin_transaction,
    api_param_assert,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitReq {
    pid: i64,
    comment: Box<str>,
    files: Vec<i64>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<SubmitReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    api::spawn_await(do_submit(db, token.0, req)).await??;

    Ok(Json(api::Response::Ok(())))
}

async fn do_submit(
    db: Database,
    token: UserToken,
    req: SubmitReq,
) -> ApiResult<(), ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        let puid =
            token.verify_joined_project(&mut trans, req.pid).await?;

        let nda_status = project_user::NdaStatus::get_by_pid_puid(
            &mut trans, req.pid, puid,
        )
        .await
        .context("project_user::NdaStatus::get_by_pid_puid")?;

        if matches!(nda_status, project_user::NdaStatus::Pending(_)) {
            api_bail_status!(
                "invalid status",
                format!("nda_status: {nda_status:?}")
            );
        }

        let (status, _) =
            project_user::Status::get_by_puid(&mut trans, puid)
                .await
                .context("project_user::Status::get_by_puid")?;

        let stage = file::Stage::try_from(status);
        let stage = if let Ok(stage) = stage
            && stage == file::Stage::Submit
        {
            stage
        } else {
            api_bail_status!(
                "invalid status",
                format!("status: {status:?}")
            );
        };
        let source = file::Source::new(req.pid, token.uid, None, stage);
        let source_pre_submit = file::Source::new(
            req.pid,
            token.uid,
            None,
            file::Stage::PreSubmit,
        );

        // expect passed pre_submit is the latest one
        // and current state is after pre submit passed
        let passed_pre_submit_id: i64 = sqlx::query_scalar(include_str!(
            "./sqls/get_last_pre_submit_by_puid.sql"
        ))
        .bind(puid)
        .fetch_one(&mut *trans)
        .await
        .context("get_last_pre_submit_by_puid")?;

        let distinct_len =
            req.files.iter().copied().collect::<HashSet<_>>().len();
        api_param_assert!(
            req.files.len() == distinct_len,
            "invalid files"
        );

        for &file_id in &req.files {
            check_file(
                &mut trans,
                file_id,
                source,
                source_pre_submit,
                passed_pre_submit_id,
            )
            .await?;
        }

        let sid: i64 =
            sqlx::query_scalar(include_str!("./sqls/ins_submit.sql"))
                .bind(puid)
                .bind(Utc::now().to_rfc3339())
                .bind(&*req.comment)
                .fetch_one(&mut *trans)
                .await
                .context("ins_submit")?;

        for &file_id in &req.files {
            file::use_file(&mut trans, file_id, stage, sid).await?;
        }

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}

async fn check_file(
    trans: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    source: file::Source,
    source_pre_submit: file::Source,
    passed_pre_submit_id: i64,
) -> Result<(), api::ApiError<ToJson>> {
    let status = file::Status::get_by_id(trans, id)
        .await
        .context("file::Status::get_by_id")?;
    let Some(status) = status else {
        api_bail_not_found!("file not found", format!("f{id} not exists"))
    };

    let (can_use, can_use_src) = if status == file::Status::Pending {
        (file::is_uploaded_by(trans, id, source).await?, "pending")
    } else if let file::Status::Used(used) = status {
        // expect a file only be used once
        // and is a passed pre submit
        if !matches!(&*used, [(file::Stage::PreSubmit, _)]) {
            api_bail_status!(
                "invalid file",
                format!(
                    "f{id} invalid previous usage of a file for submit: {used:?}"
                )
            );
        }

        if used[0].1 != passed_pre_submit_id {
            api_bail_status!(
                "invalid file",
                format!(
                    "f{id} used by invalid pre-submit, expect {}, but {}",
                    passed_pre_submit_id, used[0].1
                )
            );
        }

        (
            file::is_uploaded_by(trans, id, source_pre_submit).await?,
            "pre-submit",
        )
    } else {
        api_bail_status!(
            "invalid file",
            format!("f{id} in invalid status for submit: {status:?}")
        );
    };

    if !can_use {
        api_bail_status!(
            "invalid file",
            format!(
                "f{id} is_uploaded_by check failed, check as {can_use_src}"
            )
        )
    }

    Ok(())
}
