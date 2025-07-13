use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    S3, ServerState,
    api::{
        self, ApiResult, ToJson,
        shared::{
            file::{self, PresignedReq},
            project_user,
        },
        user::UserToken,
    },
    api_begin_transaction,
    database::Database,
    extractors::Token,
};

#[derive(Debug, Serialize, Deserialize)]
pub enum UploadFileReq {
    Start {
        pid: i64,
        name: Box<str>,
        size: i64,
        #[serde(with = "crate::utils::boxed_u8_arr_hex")]
        md5: Box<[u8; 16]>,
        #[serde(with = "crate::utils::boxed_u8_arr_hex")]
        head: Box<[u8; 12]>,
    },
    Continue {
        file_id: i64,
    },
    Finish {
        file_id: i64,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum UploadFileRes {
    UploadInfo {
        file_id: i64,
        presigned_req: PresignedReq,
    },
    Success,
    InvalidStage,
    UnusedCountReached,
    CapacityReached,
    InvalidFileType,
    InvalidFileId,
    UploadNotFinish,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<UserToken>,
    req: Json<api::Request<UploadFileReq>>,
) -> ApiResult<impl IntoResponse, ToJson> {
    let ServerState {
        mut cache, db, s3, ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    // NOTE: api_param_assert
    let upload_file_res = match req {
        UploadFileReq::Start {
            pid,
            name,
            size,
            md5,
            head,
        } => {
            api::spawn_await(do_upload_file_start(
                db, s3, token.0, pid, name, size, md5, head,
            ))
            .await??
        }
        // TODO:
        #[expect(unused, reason = "todo")]
        UploadFileReq::Continue { file_id } => unimplemented!(),
        UploadFileReq::Finish { file_id } => {
            api::spawn_await(do_upload_file_finish(
                db, s3, token.0, file_id,
            ))
            .await??
        }
    };

    Ok(Json(api::Response::Ok(upload_file_res)))
}

#[expect(clippy::too_many_arguments)]
async fn do_upload_file_start(
    db: Database,
    s3: S3,
    token: UserToken,
    pid: i64,
    name: Box<str>,
    size: i64,
    md5: Box<[u8; 16]>,
    head: Box<[u8; 12]>,
) -> ApiResult<UploadFileRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        let project_uid =
            token.verify_joined_project(&mut trans, pid).await?;

        let file_type = file::Type::detect(&head);
        let Some(file_type) = file_type else {
            tracing::debug!("unknown file type");
            return Ok(UploadFileRes::InvalidFileType);
        };
        if !file_type.is_same_as_ext(&*name) {
            tracing::debug!("file type not equal to extension");
            return Ok(UploadFileRes::InvalidFileType);
        }

        let (status, _) =
            project_user::Status::get_by_puid(&mut trans, project_uid)
                .await
                .context("project_user::Status::get_by_puid")?;

        let stage: Result<file::Stage, _> = status.try_into();

        let stage = if let Ok(stage) = stage
            && !stage.is_master()
        {
            stage
        } else {
            return Ok(UploadFileRes::InvalidStage);
        };

        let source = file::Source::new(pid, token.uid, None, stage);

        let count = file::upload_unused_file_count(&mut trans, source)
            .await
            .context("file::upload_unused_file_count")?;
        let count_limit = match stage {
            file::Stage::PreSubmit => 1,
            file::Stage::Submit => 1000,
            file::Stage::Master => unreachable!(),
        };
        if count >= count_limit {
            return Ok(UploadFileRes::UnusedCountReached);
        }

        let enough_capacity =
            file::upload_check_capacity(&mut trans, source, size)
                .await
                .context("file::upload_check_capacity")?;

        if !enough_capacity {
            return Ok(UploadFileRes::CapacityReached);
        }

        let (file_id, presigned_req) = file::upload_start(
            &mut trans, &s3, source, name, size, md5, file_type,
        )
        .await
        .context("file::upload_start")?;

        ApiResult::Ok(UploadFileRes::UploadInfo {
            file_id,
            presigned_req,
        })
    }
    .await;

    api::end_transaction(result, trans).await
}

#[expect(unused, reason = "todo")]
async fn do_upload_file_continue(
    db: Database,
    token: UserToken,
) -> ApiResult<(), ToJson> {
    // NOTE: decide the begin mode
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;

        ApiResult::Ok(())
    }
    .await;

    api::end_transaction(result, trans).await
}

async fn do_upload_file_finish(
    db: Database,
    s3: S3,
    token: UserToken,
    file_id: i64,
) -> ApiResult<UploadFileRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;

        let res = file::upload_finish(&mut trans, &s3, file_id)
            .await
            .context("upload_finish")?;

        let Some(finished) = res else {
            return Ok(UploadFileRes::InvalidFileId);
        };

        if !finished {
            return Ok(UploadFileRes::UploadNotFinish);
        }

        ApiResult::Ok(UploadFileRes::Success)
    }
    .await;

    api::end_transaction(result, trans).await
}
