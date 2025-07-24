use std::time::Duration;

use anyhow::Context as _;
use aws_sdk_s3::presigning::PresigningConfig;
use axum::{extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};

use crate::{
    S3, ServerState,
    api::{
        self, ApiResult, ToCbor, manager::ManagerToken, shared::file,
        spawn_await,
    },
    api_bail_not_found, api_begin_transaction,
    database::Database,
    extractors::{Cbor, Token},
    utils::rfc5987_utf8,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct GetFileReq {
    pid: i64,
    file_id: i64,
    r#type: GetFileType,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum GetFileType {
    Preview,
    Download,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetFileRes {
    presigned_req: file::PresignedReq,
}

pub(crate) async fn router(
    state: State<ServerState>,
    token: Token<ManagerToken>,
    req: Cbor<api::Request<GetFileReq>>,
) -> api::ApiResult<impl IntoResponse, ToCbor> {
    let ServerState {
        mut cache, db, s3, ..
    } = state.0;
    let req = req.0.verified(&mut cache).await?;

    // NOTE: api_param_assert
    let response =
        spawn_await(do_get_file(db, s3, token.0, req)).await??;

    Ok(Cbor(api::Response::Ok(response)))
}

async fn do_get_file(
    db: Database,
    s3: S3,
    token: ManagerToken,
    req: GetFileReq,
) -> ApiResult<GetFileRes, ToCbor> {
    api_begin_transaction!(db, conn, trans, Deferred);

    let result = async {
        token.verify(&mut trans).await?;
        token
            .verify_can_access_project(&mut trans, req.pid, true)
            .await?;

        // pid verified by verify_can_access_project
        let download_info =
            file::download_info(&mut trans, req.file_id, req.pid).await?;

        let Some(download_info) = download_info else {
            api_bail_not_found!(
                "file not found",
                "file not exists or mimatch project for the file"
            );
        };

        let disposition_type = match req.r#type {
            GetFileType::Preview => "inline",
            GetFileType::Download => "attachment",
        };

        let filename = download_info
            .name
            .chars()
            .filter(|it| !it.is_ascii_control())
            .collect::<String>()
            .replace('"', "\\\"");
        let filename_rfc5987 = rfc5987_utf8(download_info.name);

        let disposition = format!(
            r#"{disposition_type};
        filename="{filename}";
        filename*={filename_rfc5987}"#
        );

        let req = s3
            .get_object()
            .bucket(s3.bucket())
            .key(&*download_info.s3_key)
            .response_content_disposition(disposition)
            .presigned(
                PresigningConfig::builder()
                    .expires_in(Duration::from_secs(15 * 60))
                    .build()
                    .context("expect valid expires_in")?,
            )
            .await
            .context("presigning download request")?;

        ApiResult::Ok(GetFileRes {
            presigned_req: req.into(),
        })
    }
    .await;

    api::end_transaction(result, trans).await
}
