use std::{
    fmt::{Display, Write},
    sync::{
        Arc,
        atomic::{self, AtomicU64},
    },
};

use anyhow::Context;
use aws_config::BehaviorVersion;
use axum::{
    Router,
    body::Body,
    http::{Request, response::Parts},
};
use backend::{
    S3, ServerState, api::manager::init_root_if_not_exists, cache_init,
    database::Database, router,
};
use ciborium::cbor;
use cookie::Cookie;
use ed25519_dalek::SigningKey;
use serde_json::json;
use sha2::{Digest, Sha512};
use tempfile::NamedTempFile;
use tower::ServiceExt;
use tower_http::request_id::{MakeRequestId, RequestId};
use ulid::Ulid;

struct TestResponse {
    string: String,
    parts: Parts,
}

impl Display for TestResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.string.fmt(f)
    }
}

struct TestApp {
    router: Router,
    root_pswd: String,
    // for drop
    #[expect(dead_code)]
    db_tmp: NamedTempFile,
}

impl TestApp {
    pub async fn test_req(
        &self,
        req: Request<Body>,
        strip_hdrs: Option<&[&str]>,
    ) -> anyhow::Result<TestResponse> {
        let res = self.router.clone().oneshot(req).await?;
        let (parts, body) = res.into_parts();

        let ver =
            format!("{:?}", parts.version).trim_matches('"').to_string();
        let status = parts.status.as_u16();
        let status_msg =
            parts.status.canonical_reason().unwrap_or_default();

        let status_line = format!("{ver} {status} {status_msg}");

        let mut headers = String::new();
        for (h, v) in &parts.headers {
            let h = h.to_string();
            if let Some(strip_hdrs) = strip_hdrs
                && strip_hdrs.contains(&h.to_lowercase().as_str())
            {
                continue;
            }
            let v = v.to_str().context("invalid header value")?;

            writeln!(&mut headers, "{h}: {v}")?;
        }

        let body = axum::body::to_bytes(body, usize::MAX).await?;
        let body = body.to_vec();
        let is_utf8 = str::from_utf8(&body).is_ok();

        let body = if is_utf8 {
            String::from_utf8(body)?
        } else {
            let res = ciborium::from_reader::<ciborium::Value, _>(
                body.as_slice(),
            );

            if let Ok(res) = res {
                serde_json::to_string_pretty(&res).unwrap()
            } else {
                let mut res = String::new();
                for byte in body {
                    write!(&mut res, "{byte:0>2x}")?;
                }
                res
            }
        };

        Ok(TestResponse {
            string: format!("{status_line}\n{headers}\n{body}"),
            parts,
        })
    }
}

#[derive(Debug, Default, Clone)]
struct TestMakeRequestId(Arc<AtomicU64>);

impl MakeRequestId for TestMakeRequestId {
    fn make_request_id<B>(
        &mut self,
        _request: &axum::http::Request<B>,
    ) -> Option<RequestId> {
        let id = self.0.fetch_add(1, atomic::Ordering::SeqCst);
        Some(RequestId::new(
            Ulid::from_parts(0, (id + 1).into())
                .to_string()
                .parse()
                .unwrap(),
        ))
    }
}

async fn app() -> anyhow::Result<TestApp> {
    let key = SigningKey::from_bytes(&[
        136, 141, 156, 190, 246, 190, 57, 147, 83, 231, 221, 61, 32, 132,
        191, 56, 141, 84, 134, 182, 89, 43, 161, 46, 112, 142, 159, 62,
        15, 106, 110, 73,
    ]);

    let db_tmp = tempfile::Builder::new()
        .prefix("backend-sqlite")
        .rand_bytes(32)
        .tempfile()?;

    let db =
        Database::init(format!("sqlite://{}", db_tmp.path().display()))
            .await?;

    let cache = cache_init()
        .await
        .context("a redis compatible instance is required")?;

    let s3 = {
        let aws_config =
            aws_config::load_defaults(BehaviorVersion::latest()).await;
        let s3_config = aws_sdk_s3::config::Builder::from(&aws_config)
            .force_path_style(true)
            .build();
        let client = aws_sdk_s3::Client::from_conf(s3_config);

        S3::new(client, "main")
    };

    let state = ServerState::new(key, db, cache, s3);

    let root_pswd = init_root_if_not_exists(&state).await?.unwrap();

    let router = router(state, TestMakeRequestId::default());
    Ok(TestApp {
        router,
        root_pswd,
        db_tmp,
    })
}

#[tokio::test]
async fn user_auth() {
    let app = app().await.unwrap();

    // TODO: mock wechat api
    let res = app
        .test_req(
            Request::post(
                "http://host/api/user/wechat_login_or_register",
            )
            .header("Content-Type", "application/json")
            .body(
                json!({
                    "data": {
                        "code": "test"
                    },
                })
                .to_string()
                .into(),
            )
            .unwrap(),
            Some(&["set-cookie"]),
        )
        .await
        .unwrap();
    insta::assert_snapshot!("user_auth_login", res);

    // TODO: update name and read name

    let cookie = res.parts.headers.get("Set-Cookie").unwrap();
    let cookie = Cookie::parse_encoded(cookie.to_str().unwrap()).unwrap();

    let res = app
        .test_req(
            Request::post("http://host/api/user/revoke_all_tokens")
                .header("Content-Type", "application/json")
                .header("Cookie", cookie.stripped().to_string())
                .body(json!({ "data": null, }).to_string().into())
                .unwrap(),
            None,
        )
        .await
        .unwrap();
    insta::assert_snapshot!("user_auth_revoke_tokens", res);
}

#[tokio::test]
async fn manager_login() {
    // TODO:
    tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::TRACE)
        .init();
    let app = app().await.unwrap();

    let mut buf = vec![];

    let pswd = ciborium::Value::Bytes(
        Sha512::digest(app.root_pswd.as_bytes()).to_vec(),
    );
    ciborium::into_writer(
        &cbor!({
            "data" => {
                "Start" => {
                    "mid" => 0,
                    "password" => pswd,
                },
            },
        })
        .unwrap(),
        &mut buf,
    )
    .unwrap();

    let res = app
        .test_req(
            Request::post("http://host/api/manager/login")
                .header("Content-Type", "application/cbor")
                .body(buf.into())
                .unwrap(),
            None,
        )
        .await
        .unwrap();
    insta::assert_snapshot!(res);
}
