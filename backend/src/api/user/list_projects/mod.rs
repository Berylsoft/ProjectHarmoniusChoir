use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

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
        token.verify(&mut trans).await?;

        let projects = sqlx::query_as::<_, (i64, String)>(include_str!(
            "./sqls/list_projects.sql"
        ))
        .fetch_all(&mut *trans)
        .await
        .context("list_projects")?;

        let projects = projects
            .into_iter()
            .map(|(id, name)| Project {
                id,
                name: name.into(),
            })
            .collect_vec();

        ApiResult::<_, _>::Ok(projects)
    }
    .await;

    api::end_transaction(res, trans).await
}
