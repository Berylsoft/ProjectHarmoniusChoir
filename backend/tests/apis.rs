use std::{
    self,
    borrow::Cow,
    collections::HashMap,
    fmt::{Display, Write},
    io::Write as _,
    process::{Command, Stdio},
};

use anyhow::Context;
use axum::{
    Router,
    body::Body,
    http::{Method, Request, header::AsHeaderName, response::Parts},
};
use backend::{
    ServerState,
    api::{
        manager::{ManagerToken, init_root_if_not_exists},
        user::UserToken,
    },
    database::Database,
    init_cache, init_s3, router,
    signing::SignedData,
};
use chrono::{DateTime, TimeDelta, Utc};
use ciborium::cbor;
use cookie::{Cookie, CookieJar};
use ed25519_dalek::{SigningKey, VerifyingKey};
use itertools::Itertools;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::{Digest, Sha512};
use tempfile::NamedTempFile;
use totp_rs::TOTP;
use tower::ServiceExt;
use tower_http::request_id::{MakeRequestId, RequestId};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Default, Clone)]
struct TestMakeRequestId;

impl TestMakeRequestId {
    fn branch(&self) -> Self {
        Self
    }
}

impl MakeRequestId for TestMakeRequestId {
    fn make_request_id<B>(
        &mut self,
        _request: &axum::http::Request<B>,
    ) -> Option<RequestId> {
        Some(RequestId::new(
            "01D39ZY06FGSCTVN4T2V9PKHFZ".parse().unwrap(),
        ))
    }
}

struct TestResponse {
    parts: Parts,
    body: Vec<u8>,
}

impl TestResponse {
    fn headers(&self) -> anyhow::Result<Vec<(&str, &str)>> {
        let mut headers = self
            .parts
            .headers
            .iter()
            .map(|(h, v)| {
                Ok((h.as_str(), v.to_str().context("header value")?))
            })
            .collect::<anyhow::Result<Vec<(_, _)>>>()?;

        headers.sort();

        Ok(headers)
    }

    fn body_to_str(&self) -> anyhow::Result<&str> {
        str::from_utf8(&self.body).context("str::from_utf8")
    }

    fn body_to_hex(&self) -> String {
        let mut res = String::new();
        for byte in &self.body {
            write!(&mut res, "{byte:0>2x}").unwrap();
        }
        res
    }

    fn body_to_cbor(&self) -> anyhow::Result<ciborium::Value> {
        ciborium::from_reader::<ciborium::Value, _>(self.body.as_slice())
            .context("ciborium::from_reader")
    }

    #[expect(dead_code)]
    fn body_to_json(&self) -> anyhow::Result<serde_json::Value> {
        serde_json::from_slice::<serde_json::Value>(&self.body)
            .context("serde_json::from_slice")
    }

    fn cbor_body_to_json_string_pretty(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(&self.body_to_cbor()?)
            .context("serde_json::to_string_pretty")
    }

    fn any_body_to_string(&self) -> anyhow::Result<Cow<'_, str>> {
        let res = if let Ok(s) = self.body_to_str() {
            Cow::from(s)
        } else {
            if let Ok(res) = self.cbor_body_to_json_string_pretty() {
                res
            } else {
                self.body_to_hex()
            }
            .into()
        };

        Ok(res)
    }

    fn status_line_to_string(&self) -> String {
        let parts = &self.parts;

        let ver =
            format!("{:?}", parts.version).trim_matches('"').to_string();
        let status = parts.status.as_u16();
        let status_msg =
            parts.status.canonical_reason().unwrap_or_default();

        format!("{ver} {status} {status_msg}")
    }

    fn headers_to_string_pretty(&self) -> anyhow::Result<String> {
        let mut headers = String::new();

        for (h, v) in self.headers().context("headers")? {
            writeln!(&mut headers, "{h}: {v}")?;
        }

        Ok(headers)
    }

    fn to_string(&self) -> anyhow::Result<String> {
        let status_line = self.status_line_to_string();
        let headers = self.headers_to_string_pretty()?;
        let body = self.any_body_to_string()?;

        Ok(format!("{status_line}\n{headers}\n{body}"))
    }

    fn to_string_without_body(&self) -> anyhow::Result<String> {
        let status_line = self.status_line_to_string();
        let headers = self.headers_to_string_pretty()?;

        Ok(format!("{status_line}\n{headers}"))
    }

    fn cookies(&self) -> anyhow::Result<CookieJar> {
        let mut jar = CookieJar::new();

        let headers = self.headers()?;
        let set_cookies = headers
            .into_iter()
            .filter(|(h, _)| h.eq_ignore_ascii_case("set-cookie"))
            .collect::<HashMap<_, _>>();

        for (_, v) in set_cookies {
            let cookie = Cookie::parse_encoded(v)
                .context("Cookie::parse_encoded")?;
            jar.add_original(cookie.into_owned());
        }

        Ok(jar)
    }

    fn take_header(
        &mut self,
        key: impl AsHeaderName,
    ) -> anyhow::Result<Option<String>> {
        self.parts
            .headers
            .remove(key)
            .map(|it| it.to_str().map(|it| it.to_string()))
            .transpose()
            .map_err(Into::into)
    }

    fn take_cookies(&mut self) -> anyhow::Result<CookieJar> {
        let mut jar = CookieJar::new();

        while let Some(cookie) =
            self.take_header(axum::http::header::SET_COOKIE)?
        {
            let cookie = Cookie::parse_encoded(cookie)
                .context("Cookie::parse_encoded")?;
            jar.add_original(cookie.into_owned());
        }

        Ok(jar)
    }
}

impl Display for TestResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_string().unwrap().fmt(f)
    }
}

struct ReqBuilder<'app> {
    app: &'app mut TestApp,
    cookie_jar: CookieJar,
    cookie_store_id: u64,
    builder: axum::http::request::Builder,
}

impl<'app> ReqBuilder<'app> {
    fn api(mut self, path: impl AsRef<str>) -> Self {
        self.builder = self
            .builder
            .uri(format!("http://host/api{}", path.as_ref()));
        self
    }

    #[expect(dead_code)]
    fn cookie(
        mut self,
        key: impl Into<Cow<'static, str>>,
        value: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.cookie_jar.add(Cookie::new(key, value));
        self
    }

    #[expect(dead_code)]
    fn cookie_ref(mut self, c: &Cookie<'_>) -> Self {
        self.cookie_jar.add(c.clone().into_owned());
        self
    }

    #[expect(dead_code)]
    fn cookies(mut self, cookies: &CookieJar) -> Self {
        for cookie in cookies.iter() {
            self.cookie_jar.add(cookie.clone().into_owned());
        }
        self
    }

    fn apply_cookies(mut self) -> Self {
        let mut cookies = String::new();
        for cookie in self.cookie_jar.iter() {
            if !cookies.is_empty() {
                cookies.push(';');
            }
            cookies.push_str(&cookie.stripped().encoded().to_string());
        }

        if !cookies.is_empty() {
            self.builder = self.builder.header("Cookie", cookies)
        }

        self
    }

    async fn send_cbor(
        mut self,
        cbor: ciborium::Value,
    ) -> anyhow::Result<TestResponse> {
        self = self.apply_cookies();

        let mut buf = vec![];
        ciborium::into_writer(&cbor, &mut buf)
            .context("ciborium::into_writer")?;

        let req = self
            .builder
            .header("Content-Type", "application/cbor")
            .body(buf.into())
            .context("build request")?;

        self.app.send(req, self.cookie_store_id).await
    }

    async fn send_json(
        mut self,
        json: serde_json::Value,
    ) -> anyhow::Result<TestResponse> {
        self = self.apply_cookies();

        let req = self
            .builder
            .header("Content-Type", "application/json")
            .body(json.to_string().into())
            .context("build request")?;

        self.app.send(req, self.cookie_store_id).await
    }
}

struct TestApp {
    state: ServerState,
    cookies: HashMap<u64, CookieJar>,
    make_req_id: TestMakeRequestId,
    router: Router,
    root_pswd: String,
    // for drop
    db_tmp: NamedTempFile,
}

impl TestApp {
    fn root_pswd_sha512(&self) -> Vec<u8> {
        Sha512::digest(self.root_pswd.as_bytes()).to_vec()
    }

    fn verifying_key(&self) -> VerifyingKey {
        self.state.key.verifying_key()
    }
}

impl TestApp {
    fn new_db_tmp_file() -> anyhow::Result<NamedTempFile> {
        tempfile::Builder::new()
            .prefix("backend-sqlite")
            .suffix(".db")
            .rand_bytes(16)
            .tempfile()
            .map_err(Into::into)
    }

    async fn new() -> anyhow::Result<Self> {
        let key = SigningKey::from_bytes(&[
            136, 141, 156, 190, 246, 190, 57, 147, 83, 231, 221, 61, 32,
            132, 191, 56, 141, 84, 134, 182, 89, 43, 161, 46, 112, 142,
            159, 62, 15, 106, 110, 73,
        ]);

        let db_tmp = Self::new_db_tmp_file()?;

        dbg!(&db_tmp);
        let db = Database::init(format!(
            "sqlite://{}?mode=rwc",
            db_tmp.path().display()
        ))
        .await?;

        let cache = init_cache()
            .await
            .context("a redis compatible instance is required")?;

        let s3 = init_s3()
            .await
            .context("a s3 compatible instance is required")?;

        let state = ServerState::new(key, db, cache, s3);

        let root_pswd = init_root_if_not_exists(&state).await?.unwrap();

        let make_req_id = TestMakeRequestId;

        let router = router(state.clone(), make_req_id.clone());

        Ok(Self {
            state,
            cookies: HashMap::new(),
            make_req_id,
            router,
            root_pswd,
            db_tmp,
        })
    }

    /// only db is branched, key, cache and s3 is shared
    async fn branch(&self) -> anyhow::Result<Self> {
        let db_tmp = Self::new_db_tmp_file()?;
        std::fs::copy(self.db_tmp.path(), db_tmp.path())
            .context("copy db for branching")?;

        let db = Database::init(format!(
            "sqlite://{}?mode=rwc",
            db_tmp.path().display()
        ))
        .await
        .context("Database::init")?;

        let mut state = self.state.clone();
        state.db = db;

        let cookies = self.cookies.clone();

        let make_req_id = self.make_req_id.branch();

        let router = router(state.clone(), make_req_id.clone());

        let root_pswd = self.root_pswd.clone();

        Ok(Self {
            state,
            cookies,
            make_req_id,
            router,
            root_pswd,
            db_tmp,
        })
    }

    async fn next<Fut>(
        self,
        next: impl FnOnce(TestApp) -> Fut,
        name: &'static str,
    ) -> anyhow::Result<Self>
    where
        Fut: Future<Output = anyhow::Result<()>>,
    {
        next(self.branch().await.context("branch")?)
            .await
            .with_context(|| format!("next: {name}"))
            .map(|()| self)
    }

    async fn next_term<Fut>(
        self,
        next: impl FnOnce(TestApp) -> Fut,
        name: &'static str,
    ) -> anyhow::Result<()>
    where
        Fut: Future<Output = anyhow::Result<()>>,
    {
        next(self).await.with_context(|| format!("next: {name}"))
    }

    async fn send(
        &mut self,
        req: Request<Body>,
        cookie_store_id: u64,
    ) -> anyhow::Result<TestResponse> {
        let res = self
            .router
            .clone()
            .oneshot(req)
            .await
            .context("process request")?;
        let (parts, body) = res.into_parts();

        let body = axum::body::to_bytes(body, usize::MAX)
            .await
            .context("read body")?;
        let body = body.to_vec();

        let res = TestResponse { parts, body };

        for cookie in res.cookies()?.iter() {
            self.cookies
                .get_mut(&cookie_store_id)
                .unwrap()
                .add(cookie.clone().into_owned());
        }

        Ok(res)
    }

    pub fn req_builder(
        &mut self,
        method: Method,
        cookie_store_id: u64,
    ) -> ReqBuilder {
        let cookie_jar = self.cookies.get(&cookie_store_id);

        let cookie_jar = if let Some(cookie_jar) = cookie_jar {
            cookie_jar.clone()
        } else {
            self.cookies.insert(cookie_store_id, CookieJar::new());
            CookieJar::new()
        };

        ReqBuilder {
            cookie_jar,
            cookie_store_id,
            builder: Request::builder().method(method),
            app: self,
        }
    }

    #[expect(unused, reason = "just for quick check on db")]
    pub fn db_query(&mut self, input: &str) -> String {
        let mut cmd = Command::new("sqlite3")
            .arg(self.db_tmp.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let input = format!(
            r".mode column
{}
.quit
",
            input.replace("\t", "")
        );

        let mut stdin = cmd.stdin.take().unwrap();
        stdin.write_all(input.as_bytes()).unwrap();

        let output = cmd.wait_with_output().unwrap();
        assert!(output.status.success());

        String::from_utf8(output.stdout).unwrap()
    }
}

/// the first one is the last, other is by order
macro_rules! next {
    ($app:expr; $last:expr $(,$front:expr)* $(,)? ) => {
        $app
        $(
            .next($front, stringify!($front))
            .await?
        )*
        .next_term($last, stringify!($last))
        .await?
    };
}

fn cbor_get<'v>(
    value: &'v mut ciborium::Value,
    keys: &'static [&'static str],
) -> &'v mut ciborium::Value {
    let mut cur = value;

    for &key in keys {
        cur = cur
            .as_map_mut()
            .unwrap()
            .iter_mut()
            .find_map(|(k, v)| (k.as_text().unwrap() == key).then_some(v))
            .unwrap();
    }

    cur
}

fn cbor_remove(
    value: &mut ciborium::Value,
    keys: &'static [&'static str],
) -> ciborium::Value {
    assert!(!keys.is_empty());

    let parent = cbor_get(value, &keys[..keys.len() - 1]);

    let parent = parent.as_map_mut().unwrap();
    let (idx, _) = parent
        .iter_mut()
        .find_position(|(k, _)| {
            &k.as_text().unwrap() == keys.last().unwrap()
        })
        .unwrap();

    let (_, value) = parent.remove(idx);

    value
}

fn cbor_to_json_string_pretty(
    value: &ciborium::Value,
) -> anyhow::Result<String> {
    serde_json::to_string_pretty(value)
        .context("serde_json::to_string_pretty")
}

fn verified_token<T>(
    cookie_jar: &CookieJar,
    key: &VerifyingKey,
) -> anyhow::Result<T>
where
    T: Serialize + DeserializeOwned,
{
    let token = cookie_jar
        .get("token")
        .context("get token from cookie_jar")?;
    let token = SignedData::<T>::try_from_encoded(token.value())
        .context("SignedData::<T>::try_from_encoded")?;
    token.verify(key)
}

#[tokio::test]
async fn entry() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let app = TestApp::new().await.unwrap();

    next!(app; manager_login_start);

    Ok(())
}

async fn manager_login_start(mut app: TestApp) -> anyhow::Result<()> {
    let pswd = ciborium::Value::Bytes(app.root_pswd_sha512());
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/login")
        .send_cbor(cbor!({
            "data" => {
                "Start" => {
                    "mid" => 0,
                    "password" => pswd,
                },
            },
        })?)
        .await?;

    let mut body = res.body_to_cbor()?;
    let token = cbor_remove(&mut body, &["Ok", "TotpSetup", "token"]);
    let totp_url =
        cbor_remove(&mut body, &["Ok", "TotpSetup", "totp_url"])
            .into_text()
            .unwrap();
    let totp = TOTP::from_url(totp_url).unwrap();

    insta::assert_snapshot!(res.to_string_without_body()?, @r"
    HTTP/1.1 200 OK
    content-length: 327
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ
    ");
    insta::assert_snapshot!(cbor_to_json_string_pretty(&body)?, @r#"
    {
      "Ok": {
        "TotpSetup": {}
      }
    }
    "#);

    next!(app; async move |app| {
        manager_login_end_setup(app, token, totp).await
    });

    Ok(())
}

async fn manager_login_end_setup(
    mut app: TestApp,
    token: ciborium::Value,
    totp: TOTP,
) -> anyhow::Result<()> {
    let totp_code = totp.generate_current().unwrap().parse::<u32>()?;

    let send_time = Utc::now();
    let mut res = app
        .req_builder(Method::POST, 0)
        .api("/manager/login")
        .send_cbor(cbor!({"data" => {
            "EndSetup" => {
                "token" => token,
                "totp_code" => totp_code,
            }
        }})?)
        .await?;

    let cookies = res.take_cookies()?;
    assert_eq!(cookies.iter().count(), 1);
    let token =
        verified_token::<ManagerToken>(&cookies, &app.verifying_key())?;
    assert_eq!(token.mid, 0);
    assert!(token.expired > send_time);
    assert!(token.sudo_expired < send_time);

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 5
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": null
    }
    "#);

    // TODO: revoke then normal login
    // TODO: invalid acquire_sudo

    next!(app; async move |app| {
        let totp_code = totp.generate_current().unwrap().parse::<u32>()?;
        manager_acquire_sudo(app, totp_code, token.expired).await
    });

    Ok(())
}

async fn manager_acquire_sudo(
    mut app: TestApp,
    totp_code: u32,
    prev_expired_time: DateTime<Utc>,
) -> anyhow::Result<()> {
    let send_time = Utc::now();
    let mut res = app
        .req_builder(Method::POST, 0)
        .api("/manager/acquire_sudo")
        .send_cbor(cbor!({"data" => {
            "totp_code" => totp_code,
        }})?)
        .await?;

    let cookies = res.take_cookies()?;
    assert_eq!(cookies.iter().count(), 1);
    let token =
        verified_token::<ManagerToken>(&cookies, &app.verifying_key())?;
    assert_eq!(token.mid, 0);
    assert_eq!(token.expired, prev_expired_time);
    assert!(token.sudo_expired > send_time);

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 5
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": null
    }
    "#);

    next!(app; manager_create_project);

    Ok(())
}

async fn manager_create_project(mut app: TestApp) -> anyhow::Result<()> {
    let end_time = (Utc::now() + TimeDelta::days(7)).to_rfc3339();

    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/root/create_project")
        .send_cbor(cbor!({"data" => {
            "name"                            => "Test Project",
            "entry_question"                  => "The Question",
            "entry_answer"                    => "The Answer",
            "pre_submit_skip_password"        => "thepswd",
            "require_harmony_group_intention" => true,
            "non_disclosure_agreement"        => Some("123"),
            "attachment_key"                  => Some("test"),
            "pre_submit_file_size_min"        => 1_000_000,
            "pre_submit_file_size_max"        => 500_000_000,
            "submit_file_size_min"            => 1_000_000,
            "submit_file_size_max"            => 1_000_000_000,
            "master_file_size_max"            => 1_000_000_000,
            "end_time"                        => end_time,
        }})?)
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 5
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": null
    }
    "#);

    next!(app; user_login);

    Ok(())
}

async fn user_login(mut app: TestApp) -> anyhow::Result<()> {
    // TODO: mock wechat api
    let send_time = Utc::now();
    let mut res = app
        .req_builder(Method::POST, 1)
        .api("/user/wechat_login_or_register")
        .send_json(json!({
            "data": {
                "code": "test"
            },
        }))
        .await?;

    let cookies = res.take_cookies()?;
    assert_eq!(cookies.iter().count(), 1);
    let token =
        verified_token::<UserToken>(&cookies, &app.verifying_key())?;
    assert!(token.expired > send_time);

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 11
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {"Ok":null}
    "#);

    // TODO: update name and read name

    next!(app; user_revoke_all_tokens, user_list_projects);

    Ok(())
}

async fn user_revoke_all_tokens(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/revoke_all_tokens")
        .send_json(json!({"data": null}))
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 11
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {"Ok":null}
    "#);

    Ok(())
}

async fn user_list_projects(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/list_projects")
        .send_json(json!({"data": null}))
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 52
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {"Ok":{"projects":[{"id":1,"name":"Test Project"}]}}
    "#);

    next!(app; user_join_project_start);

    Ok(())
}

async fn user_join_project_start(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/join_project")
        .send_json(json!({"data": {
            "Start": {
                "pid": 1,
            }
        }}))
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 44
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {"Ok":{"Start":{"question":"The Question"}}}
    "#);

    next!(app; user_join_project);

    Ok(())
}

async fn user_join_project(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/join_project")
        .send_json(json!({"data": {
            "Join": {
                "pid": 1,
                "answer": "The Answer",
                "name": "TheName",
            }
        }}))
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 16
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {"Ok":"Success"}
    "#);

    // TODO: remove
    insta::assert_snapshot!(app.db_query("select * from project_users;"), @r"
    id  user_id  project_id  name   
    --  -------  ----------  -------
    1   1        1           TheName
    ");

    Ok(())
}

#[expect(unused, reason = "template")]
async fn template_manager(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("")
        .send_json(json!({"data": null}))
        .await?;

    insta::assert_snapshot!(res, @"");

    Ok(())
}

#[expect(unused, reason = "template")]
async fn template_user(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("")
        .send_cbor(cbor!({"data" => null})?)
        .await?;

    insta::assert_snapshot!(res, @"");

    Ok(())
}
