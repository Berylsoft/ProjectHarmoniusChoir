use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor, manager::ManagerToken,
        shared::project_user, spawn_await,
    },
    api_begin_transaction, api_param_assert,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListProjectUsersReq {
    pid: i64,
    #[serde(default)]
    sort_by: SortMethod,
    #[serde(default)]
    reverse: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub enum SortMethod {
    #[default]
    JoinedAt,
    Status,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListProjectUsersRes {
    project_users: Vec<ProjectUser>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectUser {
    id: i64,
    name: Box<str>,
    status: project_user::Status,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<ListProjectUsersReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    api_param_assert!(req.pid > 0);
    let project_users =
        spawn_await(do_list_project_user(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(ListProjectUsersRes {
        project_users,
    })))
}

async fn do_list_project_user(
    db: Database,
    token: ManagerToken,
    req: ListProjectUsersReq,
) -> ApiResult<Vec<ProjectUser>, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;
        token.verify_can_access_project(&mut trans, req.pid).await?;

        let project_users = sqlx::query_as::<_, (i64, String)>(
            include_str!("./sqls/get_project_users_by_pid.sql"),
        )
        .bind(req.pid)
        .fetch_all(&mut *trans)
        .await
        .context("get_project_users_by_pid")?;

        let mut result = Vec::with_capacity(project_users.len());

        for (puid, name) in project_users {
            let (status, status_at) =
                project_user::Status::get_by_puid(&mut trans, puid)
                    .await
                    .context("project_user::Status::get_by_puid")?;

            result.push((puid, name.into_boxed_str(), status, status_at));
        }

        match req.sort_by {
            SortMethod::JoinedAt => {}
            SortMethod::Status => {
                result.sort_by_key(|(_, _, status, status_at)| {
                    (*status, *status_at)
                });
            }
        }
        if req.reverse {
            result.reverse();
        }

        let result = result
            .into_iter()
            .map(|(id, name, status, _)| ProjectUser { id, name, status })
            .collect_vec();

        ApiResult::Ok(result)
    }
    .await;

    api::end_transaction(result, trans).await
}
