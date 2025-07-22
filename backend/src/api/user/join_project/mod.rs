use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{self, ApiError, ApiResult, ToJson, user::UserToken},
    api_assert, api_begin_transaction, api_param_assert,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum JoinProjectReq {
    /// query the entry question
    Start { pid: i64 },
    /// actual join
    Join { pid: i64, answer: Box<str> },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum JoinProjectRes {
    Start { question: Box<str> },
    WrongAnswer,
    AlreadyJoined,
    Success,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<JoinProjectReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    let join_project_res = match req {
        JoinProjectReq::Start { pid } => {
            api_param_assert!(!pid.is_negative());
            api::spawn_await(do_query_entry_question(db, token.0, pid))
                .await??
        }
        JoinProjectReq::Join { pid, answer } => {
            api_param_assert!(pid >= 0);
            api::spawn_await(do_join_project(db, token.0, pid, answer))
                .await??
        }
    };

    Ok(Json(api::Response::Ok(join_project_res)))
}

async fn do_query_entry_question(
    db: Database,
    token: UserToken,
    pid: i64,
) -> ApiResult<JoinProjectRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let res = async {
        token.verify(&mut trans).await?;

        let question = sqlx::query_scalar::<_, String>(include_str!(
            "./sqls/get_entry_question_by_pid.sql"
        ))
        .bind(pid)
        .fetch_optional(&mut *trans)
        .await
        .context("get_entry_question_by_pid")?;

        let Some(question) = question else {
            return Err(ApiError::BadParam {
                msg: "invalid pid".into(),
                detail: format!("project {pid} does not exists").into(),
            });
        };

        ApiResult::Ok(JoinProjectRes::Start {
            question: question.into_boxed_str(),
        })
    }
    .await;

    api::end_transaction(res, trans).await
}

async fn do_join_project(
    db: Database,
    token: UserToken,
    pid: i64,
    answer: Box<str>,
) -> ApiResult<JoinProjectRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let res = async {
        token.verify(&mut trans).await?;

        let project_user_id = sqlx::query_scalar::<_, i64>(include_str!(
            "./sqls/get_project_user_by_uid_pid.sql"
        ))
        .bind(token.uid)
        .bind(pid)
        .fetch_optional(&mut *trans)
        .await
        .context("get_project_user_by_uid_pid")?;

        if project_user_id.is_some() {
            return Ok(JoinProjectRes::AlreadyJoined);
        }

        let entry_answer = sqlx::query_scalar::<_, String>(include_str!(
            "./sqls/get_entry_answer_by_pid.sql"
        ))
        .bind(pid)
        .fetch_optional(&mut *trans)
        .await
        .context("get_entry_answer_by_pid")?;

        let Some(entry_answer) = entry_answer else {
            return Err(ApiError::BadParam {
                msg: "invalid pid".into(),
                detail: format!("project {pid} does not exists").into(),
            });
        };

        if *entry_answer != *answer {
            return Ok(JoinProjectRes::WrongAnswer);
        }

        let ins_project_user_res =
            sqlx::query(include_str!("./sqls/ins_project_user.sql"))
                .bind(token.uid)
                .bind(pid)
                .bind(Utc::now().to_rfc3339())
                .execute(&mut *trans)
                .await
                .context("ins_project_user")?;

        api_assert!(ins_project_user_res.rows_affected() == 1);

        ApiResult::Ok(JoinProjectRes::Success)
    }
    .await;

    api::end_transaction(res, trans).await
}
