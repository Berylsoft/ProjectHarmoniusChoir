use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::ManagerToken,
        shared::{file, project_user},
        spawn_await,
    },
    api_bail_not_found, api_bail_status, api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct MasterReq {
    puid: i64,
    comment: Box<str>,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<MasterReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState { mut cache, db, .. } = state.0;
    let req = req.0.verified(&mut cache).await?;

    spawn_await(do_master(db, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(())))
}

async fn do_master(
    db: Database,
    token: ManagerToken,
    req: MasterReq,
) -> ApiResult<(), ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;

        let pid_uid =
            project_user::get_pid_uid_by_id(&mut trans, req.puid).await?;

        let Some((pid, uid)) = pid_uid else {
            // ensure by client
            api_bail_not_found!(
                "project user not found",
                format!("can't find project user by puid: {}", req.puid)
            )
        };

        token
            .verify_can_access_project(&mut trans, pid, false)
            .await?;

        let mastered: i64 = sqlx::query_scalar(include_str!(
            "./sqls/is_mastered_by_puid.sql"
        ))
        .bind(req.puid)
        .fetch_one(&mut *trans)
        .await
        .context("is_mastered_by_puid")?;

        if mastered > 0 {
            api_bail_status!("already mastered");
        }

        let have_uploading =
            file::have_uploading(&mut trans, uid, Some(token.mid))
                .await?;
        if have_uploading {
            api_bail_status!("have uploading files");
        }

        let source = file::Source::new(
            pid,
            uid,
            Some(token.mid),
            file::Stage::Master,
        );

        let pending = file::Info::get_pending(&mut trans, source).await?;
        if pending.is_empty() {
            api_bail_status!("no avaiable file");
        }

        // ==================== write boundary ====================

        let id: i64 =
            sqlx::query_scalar(include_str!("./sqls/ins_master.sql"))
                .bind(req.puid)
                .bind(token.mid)
                .bind(Utc::now().to_rfc3339())
                .bind(&*req.comment)
                .fetch_one(&mut *trans)
                .await
                .context("ins_master")?;

        for info in pending {
            file::use_file(&mut trans, info.id, file::Stage::Master, id)
                .await?;
        }

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}
