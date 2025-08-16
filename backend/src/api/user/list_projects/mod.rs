use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{
    ServerState,
    api::{self, ApiResult, ToJson, user::UserToken},
    api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListProjectsRes {
    projects: Vec<Project>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    id: i64,
    name: Box<str>,
    joined: bool,
    ended: bool,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<()>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState { mut cache, db, .. } = state.0;
    req.0.verified(&mut cache).await?;

    let projects =
        api::spawn_await(do_list_projects(db, token.0)).await??;

    Ok(Json(api::Response::Ok(ListProjectsRes { projects })))
}

async fn do_list_projects(
    db: Database,
    token: UserToken,
) -> ApiResult<Vec<Project>, ToJson> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let res = async {
        #[derive(Debug, FromRow)]
        struct ProjectRow {
            id: i64,
            name: Box<str>,
            joined: i64,
            end_time: Box<str>,
        }

        token.verify(&mut trans).await?;

        let projects: Vec<ProjectRow> =
            sqlx::query_as(include_str!("./sqls/list_projects.sql"))
                .bind(token.uid)
                .fetch_all(&mut *trans)
                .await
                .context("list_projects")?;

        let now = Utc::now();

        let projects = projects
            .into_iter()
            .map(|it| {
                let end_time: DateTime<Utc> = it
                    .end_time
                    .parse()
                    .context("parse end_time from db")?;
                let ended = end_time < now;

                Ok(Project {
                    id: it.id,
                    name: it.name,
                    joined: it.joined > 0,
                    ended,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        ApiResult::<_, _>::Ok(projects)
    }
    .await;

    api::end_transaction(res, trans).await
}
