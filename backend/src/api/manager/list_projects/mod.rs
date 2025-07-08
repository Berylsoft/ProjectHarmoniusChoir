use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{self, ApiResult, ToCbor, manager::ManagerToken, spawn_await},
    api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListProjectsRes {
    projects: Vec<Project>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pid: i64,
    name: Box<str>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<()>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    req.0.verified(&mut cache).await?;

    let projects = spawn_await(do_list_projects(db, token.0)).await??;

    Ok(Cbor(api::Response::Ok(ListProjectsRes { projects })))
}

async fn do_list_projects(
    db: Database,
    token: ManagerToken,
) -> ApiResult<Vec<Project>, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;

        let projects = if token.is_root() {
            sqlx::query_as::<_, (i64, String)>(include_str!(
                "./sqls/get_all_projects.sql"
            ))
            .fetch_all(&mut *trans)
            .await
            .context("get_all_projects")?
        } else {
            sqlx::query_as::<_, (i64, String)>(include_str!(
                "./sqls/get_managed_projects_by_mid.sql"
            ))
            .bind(token.mid)
            .fetch_all(&mut *trans)
            .await
            .context("get_managed_projects_by_mid")?
        };
        let projects = projects
            .into_iter()
            .map(|(pid, name)| Project {
                pid,
                name: name.into_boxed_str(),
            })
            .collect_vec();

        ApiResult::Ok(projects)
    }
    .await;

    api::end_transaction(result, trans).await
}
