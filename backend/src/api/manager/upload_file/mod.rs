use anyhow::Context as _;
use axum::{extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    S3, ServerState,
    api::{
        self, ApiResult, ToCbor,
        manager::ManagerToken,
        shared::{
            file::{self, PresignedReq},
            project_user,
        },
        spawn_await,
    },
    api_bail_not_found, api_bail_status, api_begin_transaction,
    api_param_assert,
    database::Database,
    extractors::{Cbor, Token},
};

#[derive(Debug, Serialize, Deserialize)]
pub enum UploadFileReq {
    Start(UploadFileStart),
    ListUploading,
    Continue { file_id: i64 },
    Finish { file_id: i64 },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadFileStart {
    puid: i64,
    name: Box<str>,
    size: i64,
    #[serde(with = "crate::utils::boxed_u8_arr_hex")]
    md5: Box<[u8; 16]>,
    #[serde(with = "crate::utils::boxed_u8_arr_hex")]
    head: Box<[u8; 12]>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum UploadFileRes {
    File {
        id: i64,
        presigned_req: PresignedReq,
    },
    Success,
    InvalidFileType,
    CapacityReached,
    UploadNotFinish,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<UploadFileReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState {
        mut cache, db, s3, ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    // NOTE: api_param_assert
    let response = match req {
        UploadFileReq::Start(upload_file_start) => {
            spawn_await(do_upload_file_start(
                db,
                s3,
                token.0,
                upload_file_start,
            ))
            .await??
        }
        // TODO:
        UploadFileReq::ListUploading => todo!(),
        // TODO:
        UploadFileReq::Continue { file_id: _ } => todo!(),
        UploadFileReq::Finish { file_id } => {
            spawn_await(do_upload_file_finish(db, s3, token.0, file_id))
                .await??
        }
    };

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_upload_file_start(
    db: Database,
    s3: S3,
    token: ManagerToken,
    req: UploadFileStart,
) -> ApiResult<UploadFileRes, ToCbor> {
    api_param_assert!(file::is_valid_filename(&req.name));

    let file_type = file::Type::detect(&req.head);
    let Some(file_type) = file_type else {
        tracing::debug!("unknown file type");
        return Ok(UploadFileRes::InvalidFileType);
    };
    if !file_type.is_same_as_ext(&*req.name) {
        tracing::debug!("file type not equal to extension");
        return Ok(UploadFileRes::InvalidFileType);
    }

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

        token.verify_can_access_project(&mut trans, pid).await?;

        // expect pid valid by get_pid_uid_by_id
        let max_size: i64 = sqlx::query_scalar(include_str!(
            "./sqls/get_master_max_size_by_pid.sql"
        ))
        .bind(pid)
        .fetch_one(&mut *trans)
        .await
        .context("get_master_max_size_by_pid")?;

        // ensure by client
        api_param_assert!(req.size < max_size, "bad file size");

        let source = file::Source::new(
            pid,
            uid,
            Some(token.mid),
            file::Stage::Master,
        );

        let can_upload_new = file::upload_check_pending_file_count(
            &mut trans,
            source,
            file::Stage::Master,
        )
        .await
        .context("file::upload_check_pending_file_count")?;
        if !can_upload_new {
            // ensure by client
            api_bail_status!("pending count reached");
        }

        let enough_capacity =
            file::upload_check_capacity(&mut trans, source, req.size)
                .await
                .context("file::upload_check_capacity")?;

        if !enough_capacity {
            return Ok(UploadFileRes::CapacityReached);
        }

        let (file_id, presigned_req) = file::upload_start(
            &mut trans, &s3, source, req.name, req.size, req.md5,
            file_type,
        )
        .await
        .context("file::upload_start")?;

        ApiResult::Ok(UploadFileRes::File {
            id: file_id,
            presigned_req,
        })
    }
    .await;

    api::end_transaction(result, trans).await
}

async fn do_upload_file_finish(
    db: Database,
    s3: S3,
    token: ManagerToken,
    file_id: i64,
) -> ApiResult<UploadFileRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;

        let res = file::upload_finish(
            &mut trans,
            &s3,
            file_id,
            0,
            Some(token.mid),
        )
        .await
        .context("upload_finish")?;

        let Some(finished) = res else {
            api_bail_not_found!("file not found");
        };

        if !finished {
            return Ok(UploadFileRes::UploadNotFinish);
        }

        ApiResult::Ok(UploadFileRes::Success)
    }
    .await;

    api::end_transaction(result, trans).await
}
