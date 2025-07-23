use anyhow::Context as _;
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    S3, ServerState,
    api::{
        self, ApiResult, ToJson,
        shared::{
            file::{self, PresignedReq},
            project, project_user,
        },
        user::UserToken,
    },
    api_bail, api_bail_not_found, api_begin_transaction,
    api_param_assert,
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
    List,
    Continue {
        file_id: i64,
    },
    Finish {
        file_id: i64,
    },
}

// TODO: convert some error cause by client implementation issue to bad_param or else
#[derive(Debug, Serialize, Deserialize)]
pub enum UploadFileRes {
    UploadInfo {
        file_id: i64,
        presigned_req: PresignedReq,
    },
    List {
        files: Vec<file::Info>,
    },
    Continue {
        presigned_req: PresignedReq,
    },
    Success,
    InvalidStage,
    CountReached,
    CapacityReached,
    InvalidFileName,
    InvalidFileType,
    InvalidFile,
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
        UploadFileReq::List => {
            api::spawn_await(do_upload_file_list(db, token.0)).await??
        }
        UploadFileReq::Continue { file_id } => {
            api::spawn_await(do_upload_file_continue(
                db, s3, token.0, file_id,
            ))
            .await??
        }
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
    api_param_assert!(file::is_valid_filename(&name));

    let file_type = file::Type::detect(&head);
    let Some(file_type) = file_type else {
        tracing::debug!("unknown file type");
        return Ok(UploadFileRes::InvalidFileType);
    };
    if !file_type.is_same_as_ext(&*name) {
        tracing::debug!("file type not equal to extension");
        return Ok(UploadFileRes::InvalidFileType);
    }

    api_begin_transaction!(db, conn, trans, Immediate);

    let result = async {
        token.verify(&mut trans).await?;
        let project_uid =
            token.verify_joined_project(&mut trans, pid).await?;

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

        let info = project::Info::get_by_id(&mut trans, pid)
            .await
            .context("project::Info::get_by_id")?
            .context("pid verified by verify_joined_project")?;
        // expect client implementation check this before sending request
        if stage.is_pre_submit() {
            api_param_assert!(
                (info.pre_submit_file_size_min
                    ..=info.pre_submit_file_size_max)
                    .contains(&size),
                "bad file size"
            );
        } else if stage.is_submit() {
            api_param_assert!(
                (info.submit_file_size_min..=info.submit_file_size_max)
                    .contains(&size),
                "bad file size"
            );
        } else {
            api_bail!("unreachable");
        }

        let source = file::Source::new(pid, token.uid, None, stage);

        let enough = file::upload_check_pending_file_count(
            &mut trans, source, stage,
        )
        .await
        .context("file::upload_check_pending_file_count")?;
        if !enough {
            return Ok(UploadFileRes::CountReached);
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

async fn do_upload_file_list(
    db: Database,
    token: UserToken,
) -> ApiResult<UploadFileRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;

        let files =
            file::upload_list_uploading(&mut trans, token.uid, None)
                .await?;

        ApiResult::Ok(UploadFileRes::List { files })
    }
    .await;

    api::end_transaction(result, trans).await
}

async fn do_upload_file_continue(
    db: Database,
    s3: S3,
    token: UserToken,
    file_id: i64,
) -> ApiResult<UploadFileRes, ToJson> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;

        let presigned_req = file::upload_continue(
            &mut trans, &s3, file_id, token.uid, None,
        )
        .await
        .context("file::upload_continue")?;

        let Some(presigned_req) = presigned_req else {
            api_bail_not_found!(
                "file not found",
                format!(
                    "file {file_id} not exists or not upload by user {}",
                    token.uid
                )
            );
        };

        ApiResult::Ok(UploadFileRes::Continue { presigned_req })
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

        let res = file::upload_finish(
            &mut trans, &s3, file_id, token.uid, None,
        )
        .await
        .context("upload_finish")?;

        let Some(finished) = res else {
            return Ok(UploadFileRes::InvalidFile);
        };

        if !finished {
            return Ok(UploadFileRes::UploadNotFinish);
        }

        ApiResult::Ok(UploadFileRes::Success)
    }
    .await;

    api::end_transaction(result, trans).await
}
