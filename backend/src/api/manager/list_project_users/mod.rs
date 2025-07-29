use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use chrono::{DateTime, Utc};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::ManagerToken,
        shared::{
            project_user,
            submit::{self, GroupInfo},
        },
        spawn_await,
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
    uid: i64,
    status: project_user::Status,
    name: Option<Box<str>>,
    group_info: Option<submit::GroupInfo>,
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
        #[derive(Debug, FromRow)]
        struct ProjectUserRow {
            id: i64,
            user_id: i64,
        }

        #[derive(Debug)]
        struct ProjectUserSort {
            id: i64,
            user_id: i64,
            status: project_user::Status,
            status_at: DateTime<Utc>,
            name: Option<Box<str>>,
            group_info: Option<GroupInfo>,
        }

        token.verify(&mut trans).await?;
        token
            .verify_can_access_project(&mut trans, req.pid, true)
            .await?;

        let project_users: Vec<ProjectUserRow> = sqlx::query_as(
            include_str!("./sqls/get_project_users_by_pid.sql"),
        )
        .bind(req.pid)
        .fetch_all(&mut *trans)
        .await
        .context("get_project_users_by_pid")?;

        let mut result = Vec::with_capacity(project_users.len());

        for pu in project_users {
            let (status, status_at) =
                project_user::Status::get_by_puid(&mut trans, pu.id)
                    .await
                    .context("project_user::Status::get_by_puid")?;

            let name_group_info: Option<(Box<str>, GroupInfo)> =
                if status >= project_user::Status::PreSubmitPassed {
                    Some((
                        project_user::get_name_by_id(&mut trans, pu.id)
                            .await?,
                        submit::GroupInfo::get_by_puid(&mut trans, pu.id)
                            .await?,
                    ))
                } else {
                    None
                };

            let (name, group_info) = name_group_info.unzip();

            result.push(ProjectUserSort {
                id: pu.id,
                user_id: pu.user_id,
                status,
                status_at,
                name,
                group_info,
            });
        }

        match req.sort_by {
            SortMethod::JoinedAt => {}
            SortMethod::Status => {
                result.sort_by_key(|pu| (pu.status, pu.status_at));
            }
        }
        if req.reverse {
            result.reverse();
        }

        let result = result
            .into_iter()
            .map(|pu| ProjectUser {
                id: pu.id,
                uid: pu.user_id,
                status: pu.status,
                name: pu.name,
                group_info: pu.group_info,
            })
            .collect_vec();

        ApiResult::Ok(result)
    }
    .await;

    api::end_transaction(result, trans).await
}
