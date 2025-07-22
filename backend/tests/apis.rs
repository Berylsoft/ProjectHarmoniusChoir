use std::{
    self,
    any::{Any, TypeId},
    borrow::Cow,
    collections::{HashMap, VecDeque},
    fmt::{Display, Write},
    io::Write as _,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Instant,
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
use hex::ToHex;
use humantime::format_duration;
use itertools::Itertools;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
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

    fn body_to_json(&self) -> anyhow::Result<serde_json::Value> {
        serde_json::from_slice::<serde_json::Value>(&self.body)
            .context("serde_json::from_slice")
    }

    fn cbor_body_to_json_string_pretty(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(&self.body_to_cbor()?)
            .context("serde_json::to_string_pretty")
    }

    fn json_body_to_string_pretty(&self) -> anyhow::Result<String> {
        serde_json::to_string_pretty(&self.body_to_json()?)
            .context("serde_json::to_string_pretty")
    }

    fn any_body_to_string(&self) -> anyhow::Result<Cow<'_, str>> {
        let res = if let Ok(s) = self.json_body_to_string_pretty() {
            Cow::from(s)
        } else if let Ok(s) = self.body_to_str() {
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

    fn to_string_with_body<T: Serialize>(
        &self,
        body: &T,
    ) -> anyhow::Result<String> {
        Ok(format!(
            "{}\n{}",
            self.to_string_without_body()?,
            to_string_pretty(body)?
        ))
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

struct Task {
    name: String,
    fut: Pin<Box<dyn Future<Output = anyhow::Result<()>> + 'static>>,
}

type TaskQueue = Rc<Mutex<VecDeque<Task>>>;

struct TestApp {
    queue: TaskQueue,

    state: ServerState,
    cookies: HashMap<u64, CookieJar>,
    make_req_id: TestMakeRequestId,
    router: Router,
    storage: HashMap<(&'static str, TypeId), Arc<dyn Any>>,
    // for drop
    db_tmp: NamedTempFile,
}

impl TestApp {
    fn root_pswd_sha512(&self) -> Vec<u8> {
        Sha512::digest(self.get::<Box<str>>("root_pswd").as_bytes())
            .to_vec()
    }

    fn set<T: 'static>(&mut self, k: &'static str, v: T) {
        self.storage.insert((k, v.type_id()), Arc::from(v));
    }

    fn get<T: 'static>(&self, k: &'static str) -> &T {
        self.storage
            .get(&(k, TypeId::of::<T>()))
            .unwrap()
            .downcast_ref()
            .unwrap()
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

    async fn new(queue: TaskQueue) -> anyhow::Result<Self> {
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

        let s3 = {
            let s3 = init_s3()
                .await
                .context("a s3 compatible instance is required")?;

            s3.put_object()
                .bucket(s3.bucket())
                .key("test")
                .send()
                .await
                .context("create attachment test file")?;

            s3
        };

        let state = ServerState::new(key, db, cache, s3);

        let root_pswd = init_root_if_not_exists(&state).await?.unwrap();

        let make_req_id = TestMakeRequestId;

        let router = router(state.clone(), make_req_id.clone());

        let mut this = Self {
            queue,

            state,
            cookies: HashMap::new(),
            make_req_id,
            router,
            storage: HashMap::new(),
            db_tmp,
        };

        this.set::<Box<str>>("root_pswd", root_pswd);

        Ok(this)
    }

    /// only db is branched, key, cache and s3 is shared
    async fn branch(&self) -> anyhow::Result<Self> {
        let queue = Rc::clone(&self.queue);

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

        let storage = self.storage.clone();

        Ok(Self {
            queue,

            state,
            cookies,
            make_req_id,
            router,
            storage,
            db_tmp,
        })
    }

    async fn next<Fut>(
        self,
        next: impl FnOnce(TestApp) -> Fut,
        name: &'static str,
    ) -> anyhow::Result<Self>
    where
        Fut: Future<Output = anyhow::Result<()>> + 'static,
    {
        let app = self.branch().await.context("branch")?;
        self.queue.lock().unwrap().push_back(Task {
            name: name.into(),
            fut: Box::pin(next(app)),
        });
        Ok(self)
    }

    async fn next_term<Fut>(
        self,
        next: impl FnOnce(TestApp) -> Fut,
        name: &'static str,
    ) -> anyhow::Result<()>
    where
        Fut: Future<Output = anyhow::Result<()>> + 'static,
    {
        let queue = Rc::clone(&self.queue);
        queue.lock().unwrap().push_back(Task {
            name: name.into(),
            fut: Box::pin(next(self)),
        });
        Ok(())
    }

    async fn send(
        &mut self,
        req: Request<Body>,
        cookie_store_id: u64,
    ) -> anyhow::Result<TestResponse> {
        let uri = req.uri().clone();
        let start = Instant::now();
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
        let wall = start.elapsed();

        let body = body.to_vec();

        let res = TestResponse { parts, body };

        for cookie in res.cookies()?.iter() {
            self.cookies
                .get_mut(&cookie_store_id)
                .unwrap()
                .add(cookie.clone().into_owned());
        }

        tracing::info!(
            "request to {uri} finished, wall: {}",
            format_duration(wall)
        );

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
    pub fn db_query_prt(&mut self, input: &str) {
        let mut cmd = Command::new("sqlite3")
            .arg(self.db_tmp.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let input = format!(
            r".mode column
.echo on
{}
.quit
",
            input.replace("\t", "")
        );

        let mut stdin = cmd.stdin.take().unwrap();
        stdin.write_all(input.as_bytes()).unwrap();

        let output = cmd.wait_with_output().unwrap();
        assert!(output.status.success());

        let output = output.stdout;

        if let Ok(output) = str::from_utf8(&output) {
            println!("{output}");
        } else {
            std::fs::write("test_db_query_out.bin", output).unwrap();
        }
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
        if let Ok(idx) = key.parse::<usize>() {
            cur = cur.as_array_mut().unwrap().get_mut(idx).unwrap()
        } else {
            cur = cur
                .as_map_mut()
                .unwrap()
                .iter_mut()
                .find_map(|(k, v)| {
                    (k.as_text().unwrap() == key).then_some(v)
                })
                .unwrap();
        }
    }

    cur
}

fn cbor_remove(
    value: &mut ciborium::Value,
    keys: &'static [&'static str],
) -> ciborium::Value {
    assert!(!keys.is_empty());

    let parent = cbor_get(value, &keys[..keys.len() - 1]);

    let key = *keys.last().unwrap();

    if let Ok(idx) = key.parse::<usize>() {
        parent.as_array_mut().unwrap().remove(idx)
    } else {
        let parent = parent.as_map_mut().unwrap();
        let (idx, _) = parent
            .iter_mut()
            .find_position(|(k, _)| k.as_text().unwrap() == key)
            .unwrap();
        parent.remove(idx).1
    }
}

fn json_get<'v>(
    value: &'v mut serde_json::Value,
    keys: &'static [&'static str],
) -> &'v mut serde_json::Value {
    let mut cur = value;

    for &key in keys {
        cur = cur.as_object_mut().unwrap().get_mut(key).unwrap();
    }

    cur
}

fn json_remove(
    value: &mut serde_json::Value,
    keys: &'static [&'static str],
) -> serde_json::Value {
    assert!(!keys.is_empty());

    let parent = json_get(value, &keys[..keys.len() - 1]);

    let parent = parent.as_object_mut().unwrap();

    parent.remove(*keys.last().unwrap()).unwrap()
}

fn to_string_pretty<T: Serialize>(value: &T) -> anyhow::Result<String> {
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

    let queue = Rc::new(Mutex::new(VecDeque::new()));

    let app = TestApp::new(Rc::clone(&queue)).await.unwrap();

    next!(app; manager_login_start);

    loop {
        let Some(task) = queue.lock().unwrap().pop_front() else {
            break;
        };

        let Task { name, fut } = task;

        tracing::info!("running {name}");
        let start = Instant::now();
        fut.await.with_context(|| format!("running: {name}"))?;
        let elapsed = start.elapsed();
        tracing::info!("{name} finished in {}", format_duration(elapsed));
    }

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

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 327
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

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
            "non_disclosure_agreement"        => Some("the nda"),
            "attachment_key"                  => Some("test"),
            "pre_submit_file_size_min"        => 100,
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

    next!(app; manager_create_manager);

    Ok(())
}

async fn manager_create_manager(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/root/create_manager")
        .send_cbor(cbor!({"data" => null})?)
        .await?;

    let mut body = res.body_to_cbor()?;
    let mid: i128 = cbor_get(&mut body, &["Ok", "mid"])
        .as_integer()
        .unwrap()
        .into();
    let mid = mid as i64;
    let password = cbor_remove(&mut body, &["Ok", "password"]);
    let password = password.into_text().unwrap().into_boxed_str();

    app.set::<i64>("manager_1_mid", mid);
    app.set::<Box<str>>("manager_1_password", password);

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 53
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "mid": 1
      }
    }
    "#);

    // TODO: login using this manager

    next!(app; manager_project_manager_edit);

    Ok(())
}

async fn manager_project_manager_edit(
    mut app: TestApp,
) -> anyhow::Result<()> {
    let mid = *app.get::<i64>("manager_1_mid");

    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/root/project_manager_edit")
        .send_cbor(cbor!({"data" => {
            "pid" => 1,
            "mid" => mid,
            "is_revoke" => false,
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

    {
      "Ok": null
    }
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

    {
      "Ok": null
    }
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

    {
      "Ok": {
        "projects": [
          {
            "id": 1,
            "name": "Test Project"
          }
        ]
      }
    }
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

    {
      "Ok": {
        "Start": {
          "question": "The Question"
        }
      }
    }
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
            }
        }}))
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 16
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": "Success"
    }
    "#);

    next!(app; manager_list_projects);

    Ok(())
}

async fn manager_list_projects(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/list_projects")
        .send_cbor(cbor!({"data" => null})?)
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 39
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "projects": [
          {
            "pid": 1,
            "name": "Test Project"
          }
        ]
      }
    }
    "#);

    // TODO: by non root manager

    next!(app; manager_list_project_users);

    Ok(())
}

async fn manager_list_project_users(
    mut app: TestApp,
) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/list_project_users")
        .send_cbor(cbor!({"data" => {
            "pid" => 1,
            "sort_by" => "Status",
            "reverse" => false,
        }})?)
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 58
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "project_users": [
          {
            "id": 1,
            "status": "Entered",
            "name": null,
            "group_info": null
          }
        ]
      }
    }
    "#);

    next!(app; user_project_info);

    Ok(())
}

async fn user_project_info(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/project_info")
        .send_json(json!({"data": {
            "pid": 1
        }}))
        .await?;

    let mut body = res.body_to_json()?;
    let end_time = json_remove(&mut body, &["Ok", "info", "end_time"])
        .as_str()
        .unwrap()
        .to_string();
    let _: DateTime<Utc> = end_time.parse()?;

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 333
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "info": {
          "name": "Test Project",
          "pre_submit_file_size_max": 500000000,
          "pre_submit_file_size_min": 100,
          "require_harmony_group_intention": true,
          "submit_file_size_max": 1000000000,
          "submit_file_size_min": 1000000
        },
        "nda_info": null,
        "pre_submit_detail": null,
        "status": "Entered",
        "submit_detail": null
      }
    }
    "#);

    next!(app; user_upload_file_start);

    Ok(())
}

async fn user_upload_file_start(mut app: TestApp) -> anyhow::Result<()> {
    let test_file = include_bytes!("./apis/test.aac")
        .iter()
        .copied()
        .pad_using(100, |_| 0)
        .collect_vec()
        .into_boxed_slice();
    app.set::<Box<[u8]>>("test_file::data", test_file.clone());

    let md5_hex = format!("{:x}", md5::compute(&test_file));
    let mut head = Box::new([0_u8; 12]);
    let copy_len = test_file.len().min(12);
    head[..copy_len].copy_from_slice(&test_file[..copy_len]);

    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/upload_file")
        .send_json(json!({"data": {
            "Start": {
                "pid": 1,
                "name": "test.aac",
                "size": test_file.len(),
                "md5": md5_hex,
                "head": head.encode_hex::<String>(),
            }
        }}))
        .await?;

    let mut body = res.body_to_json()?;

    let file_id = json_get(&mut body, &["Ok", "UploadInfo", "file_id"])
        .as_i64()
        .unwrap();
    app.set::<i64>("test_file::file_id", file_id);

    let presigned_req =
        json_get(&mut body, &["Ok", "UploadInfo", "presigned_req"]);
    let uri = json_remove(presigned_req, &["uri"])
        .as_str()
        .unwrap()
        .to_string();

    #[derive(Deserialize)]
    struct PresignedReq {
        method: String,
        headers: Vec<(String, String)>,
    }
    let presigned_req: PresignedReq =
        serde_json::from_value(presigned_req.clone())?;
    assert_eq!(presigned_req.method, "PUT");

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 582
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "UploadInfo": {
          "file_id": 1,
          "presigned_req": {
            "headers": [
              [
                "content-length",
                "100"
              ],
              [
                "content-md5",
                "XXP6UYU9zlv6g+RDAH4LkA=="
              ],
              [
                "content-type",
                "audio/aac"
              ]
            ],
            "method": "PUT"
          }
        }
      }
    }
    "#);

    let headers = {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
        let mut headers = HeaderMap::new();
        for (k, v) in presigned_req.headers {
            headers.insert(
                HeaderName::from_str(&k)?,
                HeaderValue::from_str(&v)?,
            );
        }
        headers
    };

    let res = reqwest::Client::new()
        .put(uri)
        .headers(headers)
        .body(test_file.to_vec())
        .send()
        .await?;
    insta::assert_snapshot!(res.status(), @"200 OK");

    next!(app; user_upload_file_finish);

    Ok(())
}

async fn user_upload_file_finish(mut app: TestApp) -> anyhow::Result<()> {
    let file_id = *app.get::<i64>("test_file::file_id");

    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/upload_file")
        .send_json(json!({"data": {
            "Finish": {
                "file_id": file_id
            }
        }}))
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 16
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": "Success"
    }
    "#);

    next!(app; user_pre_submit);

    Ok(())
}

async fn user_pre_submit(mut app: TestApp) -> anyhow::Result<()> {
    let file_id = *app.get::<i64>("test_file::file_id");

    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/pre_submit")
        .send_json(json!({"data": {
            "pid": 1,
            "name": "TheName",
            "harmony_group_intention": true,
            "comment": "The Comment",
            "file_id": file_id,
        }}))
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 16
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": "Success"
    }
    "#);

    next!(app; manager_pre_submit_info);

    Ok(())
}

async fn manager_pre_submit_info(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/pre_submit_info")
        .send_cbor(cbor!({"data" => {
            "puid" => 1,
        }})?)
        .await?;

    let mut body = res.body_to_cbor()?;
    let created_at =
        cbor_remove(&mut body, &["Ok", "pre_submits", "0", "created_at"])
            .into_text()
            .unwrap();
    let created_at: DateTime<Utc> = created_at.parse()?;
    assert!(created_at <= Utc::now());

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 158
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "pre_submits": [
          {
            "id": 1,
            "name": "TheName",
            "harmony_group_intention": true,
            "comment": "The Comment",
            "file_info": {
              "id": 1,
              "name": "test.aac"
            },
            "status": null
          }
        ]
      }
    }
    "#);

    next!(app; manager_get_file);

    Ok(())
}

async fn manager_get_file(mut app: TestApp) -> anyhow::Result<()> {
    let file_id = *app.get::<i64>("test_file::file_id");

    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/get_file")
        .send_cbor(cbor!({"data" => {
            "pid" => 1,
            "file_id" => file_id,
            "type" => "Preview",
        }})?)
        .await?;

    let mut body = res.body_to_cbor()?;
    let presigned_req = cbor_get(&mut body, &["Ok", "presigned_req"]);
    let uri = cbor_remove(presigned_req, &["uri"]).into_text().unwrap();
    let method = cbor_get(presigned_req, &["method"]).as_text().unwrap();
    tracing::info!("download uri: {uri}");

    assert_eq!(method, "GET");

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 552
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "presigned_req": {
          "method": "GET",
          "headers": []
        }
      }
    }
    "#);

    let res = reqwest::Client::new().get(uri).send().await?;
    insta::assert_snapshot!(res.status(), @"200 OK");

    let data = res.bytes().await?.to_vec();
    let file_data = app.get::<Box<[u8]>>("test_file::data");
    assert_eq!(**file_data, data);

    next!(app; manager_pre_submit_review);

    Ok(())
}

async fn manager_pre_submit_review(
    mut app: TestApp,
) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/pre_submit_review")
        .send_cbor(cbor!({"data" => {
            "pid" => 1,
            "sid" => 1,
            "status" => {
                "Passed" => {
                    "lead" => false,
                    "choir" => true,
                    "harmony" => true,
                }
            }
        }})?)
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 12
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": "Success"
    }
    "#);

    next!(app; manager_list_project_users_after_pre_submit_passed);

    Ok(())
}

async fn manager_list_project_users_after_pre_submit_passed(
    mut app: TestApp,
) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/list_project_users")
        .send_cbor(cbor!({"data" => {
            "pid" => 1,
            "sort_by" => "Status",
            "reverse" => false,
        }})?)
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 95
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "project_users": [
          {
            "id": 1,
            "status": "PreSubmitPassed",
            "name": "TheName",
            "group_info": {
              "lead": false,
              "choir": true,
              "harmony": true
            }
          }
        ]
      }
    }
    "#);

    next!(app; user_project_info_after_pre_submit_passed);

    Ok(())
}

async fn user_project_info_after_pre_submit_passed(
    mut app: TestApp,
) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/project_info")
        .send_json(json!({"data": {
            "pid": 1
        }}))
        .await?;

    let mut body = res.body_to_json()?;
    let end_time = json_remove(&mut body, &["Ok", "info", "end_time"])
        .as_str()
        .unwrap()
        .to_string();
    let _: DateTime<Utc> = end_time.parse()?;

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 407
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "info": {
          "name": "Test Project",
          "pre_submit_file_size_max": 500000000,
          "pre_submit_file_size_min": 100,
          "require_harmony_group_intention": true,
          "submit_file_size_max": 1000000000,
          "submit_file_size_min": 1000000
        },
        "nda_info": {
          "Pending": "the nda"
        },
        "pre_submit_detail": {
          "Passed": {
            "choir": true,
            "harmony": true,
            "lead": false
          }
        },
        "status": "PreSubmitPassed",
        "submit_detail": null
      }
    }
    "#);

    next!(app; user_agree_nda);

    Ok(())
}

async fn user_agree_nda(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/agree_nda")
        .send_json(json!({"data": {
            "pid": 1,
        }}))
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 11
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": null
    }
    "#);

    next!(app; user_submit);

    Ok(())
}

async fn user_submit(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/submit")
        .send_json(json!({"data": {
            "pid": 1,
            "comment": "some comment for submit, or maybe not",
            "files": [1],
        }}))
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 11
    content-type: application/json
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": null
    }
    "#);

    next!(app; manager_submit_info);

    Ok(())
}

async fn manager_submit_info(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/submit_info")
        .send_cbor(cbor!({"data" => {
            "puid" => 1,
        }})?)
        .await?;

    let mut body = res.body_to_cbor()?;
    let created_at =
        cbor_remove(&mut body, &["Ok", "submits", "0", "created_at"])
            .into_text()
            .unwrap();
    let created_at: DateTime<Utc> = created_at.parse()?;
    assert!(created_at <= Utc::now());

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 155
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "submits": [
          {
            "id": 1,
            "comment": "some comment for submit, or maybe not",
            "files": [
              {
                "id": 1,
                "name": "test.aac"
              }
            ],
            "status": null
          }
        ],
        "checked_files": []
      }
    }
    "#);

    next!(app; manager_submit_review);

    Ok(())
}

async fn manager_submit_review(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/submit_review")
        .send_cbor(cbor!({"data" => {
            "pid" => 1,
            "sid" => 1,
            "status" => "Passed",
            "checked_files" => [1],
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

    next!(app; manager_upload_file_start);

    Ok(())
}

async fn manager_upload_file_start(
    mut app: TestApp,
) -> anyhow::Result<()> {
    let test_file = app.get::<Box<[u8]>>("test_file::data").clone();

    let md5_hex = format!("{:x}", md5::compute(&test_file));
    let mut head = Box::new([0_u8; 12]);
    let copy_len = test_file.len().min(12);
    head[..copy_len].copy_from_slice(&test_file[..copy_len]);

    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/upload_file")
        .send_cbor(cbor!({"data" => {
            "Start" => {
                "puid" => 1,
                "name" => "test.aac",
                "size" => test_file.len(),
                "md5" => md5_hex,
                "head" => head.encode_hex::<String>(),
            }
        }})?)
        .await?;

    let mut body = res.body_to_cbor()?;

    let file_id: i128 = cbor_get(&mut body, &["Ok", "File", "id"])
        .as_integer()
        .unwrap()
        .into();
    let file_id = file_id as i64;
    app.set::<i64>("test_file::master::file_id", file_id);

    let presigned_req =
        cbor_get(&mut body, &["Ok", "File", "presigned_req"]);
    let uri = cbor_remove(presigned_req, &["uri"])
        .as_text()
        .unwrap()
        .to_string();

    #[derive(Deserialize)]
    struct PresignedReq {
        method: String,
        headers: Vec<(String, String)>,
    }
    let presigned_req: PresignedReq = presigned_req.deserialized()?;
    assert_eq!(presigned_req.method, "PUT");

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 533
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "File": {
          "id": 2,
          "presigned_req": {
            "method": "PUT",
            "headers": [
              [
                "content-length",
                "100"
              ],
              [
                "content-md5",
                "XXP6UYU9zlv6g+RDAH4LkA=="
              ],
              [
                "content-type",
                "audio/aac"
              ]
            ]
          }
        }
      }
    }
    "#);

    let headers = {
        use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
        let mut headers = HeaderMap::new();
        for (k, v) in presigned_req.headers {
            headers.insert(
                HeaderName::from_str(&k)?,
                HeaderValue::from_str(&v)?,
            );
        }
        headers
    };

    let res = reqwest::Client::new()
        .put(uri)
        .headers(headers)
        .body(test_file.to_vec())
        .send()
        .await?;
    insta::assert_snapshot!(res.status(), @"200 OK");

    next!(app; manager_upload_file_finish);

    Ok(())
}

async fn manager_upload_file_finish(
    mut app: TestApp,
) -> anyhow::Result<()> {
    let file_id = *app.get::<i64>("test_file::master::file_id");

    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/upload_file")
        .send_cbor(cbor!({"data" => {
            "Finish" => {
                "file_id" => file_id,
            }
        }})?)
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 12
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": "Success"
    }
    "#);

    next!(app; manager_master);

    Ok(())
}

async fn manager_master(mut app: TestApp) -> anyhow::Result<()> {
    let file_id = *app.get::<i64>("test_file::master::file_id");

    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/master")
        .send_cbor(cbor!({"data" => {
            "puid" => 1,
            "file_id" => file_id,
            "comment" => "some comment for master, or maybe empty",
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

    next!(app; manager_master_info);

    Ok(())
}

async fn manager_master_info(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/master_info")
        .send_cbor(cbor!({"data" => {
            "puid" => 1,
        }})?)
        .await?;

    let mut body = res.body_to_cbor()?;
    let created_at = cbor_remove(&mut body, &["Ok", "created_at"]);
    let _: DateTime<Utc> = created_at.into_text().unwrap().parse()?;

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 99
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "mid": 0,
        "comment": "some comment for master, or maybe empty"
      }
    }
    "#);

    next!(app; manager_bundle_job_submit);

    Ok(())
}

async fn manager_bundle_job_submit(
    mut app: TestApp,
) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/bundle_job")
        .send_cbor(cbor!({"data" => {
            "Submit" => {
                "pid" => 1,
                "puids" => [1],
            }
        }})?)
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 12
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": "Success"
    }
    "#);

    next!(app; manager_bundle_job_list);

    Ok(())
}

async fn manager_bundle_job_list(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/bundle_job")
        .send_cbor(cbor!({"data" => {
            "List" => {
                "pid" => 1,
            }
        }})?)
        .await?;

    insta::assert_snapshot!(res, @r#"
    HTTP/1.1 200 OK
    content-length: 32
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "Jobs": {
          "jobs": [
            {
              "id": 1,
              "finished": false
            }
          ]
        }
      }
    }
    "#);

    next!(app; manager_bundle_job_download);

    Ok(())
}

async fn manager_bundle_job_download(
    mut app: TestApp,
) -> anyhow::Result<()> {
    let mut jobs = app.state.pending_jobs.lock().await;
    let job = jobs.get_mut(&1).unwrap().wait.take().unwrap();
    drop(jobs);
    let _ = job.await;

    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/bundle_job")
        .send_cbor(cbor!({"data" => {
            "Download" => {
                "job_id" => 1,
            }
        }})?)
        .await?;

    let mut body = res.body_to_cbor()?;
    let presigned_req =
        cbor_get(&mut body, &["Ok", "Download", "presigned_req"]);
    let uri = cbor_remove(presigned_req, &["uri"]).into_text().unwrap();
    let method = cbor_get(presigned_req, &["method"]).as_text().unwrap();
    tracing::info!("download uri: {uri}");

    assert_eq!(method, "GET");

    insta::assert_snapshot!(res.to_string_with_body(&body)?, @r#"
    HTTP/1.1 200 OK
    content-length: 375
    content-type: application/cbor
    x-request-id: 01D39ZY06FGSCTVN4T2V9PKHFZ

    {
      "Ok": {
        "Download": {
          "presigned_req": {
            "method": "GET",
            "headers": []
          }
        }
      }
    }
    "#);

    let res = reqwest::Client::new().get(uri).send().await?;
    insta::assert_snapshot!(res.status(), @"200 OK");

    let data = res.bytes().await?.to_vec();
    let file_data = app.get::<Box<[u8]>>("test_file::data");
    assert!(data.len() > file_data.len());

    Ok(())
}

#[expect(unused, reason = "template")]
async fn template_manager(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 0)
        .api("/manager/")
        .send_cbor(cbor!({"data" => null})?)
        .await?;

    // NOTE: choose one
    insta::assert_snapshot!(res, @"");
    // NOTE: choose one
    let mut body = res.body_to_cbor()?;
    insta::assert_snapshot!(res.to_string_with_body(&body)?, @"");

    Ok(())
}

#[expect(unused, reason = "template")]
async fn template_user(mut app: TestApp) -> anyhow::Result<()> {
    let res = app
        .req_builder(Method::POST, 1)
        .api("/user/")
        .send_json(json!({"data": null}))
        .await?;

    // NOTE: choose one
    insta::assert_snapshot!(res, @"");
    // NOTE: choose one
    let mut body = res.body_to_json()?;
    insta::assert_snapshot!(res.to_string_with_body(&body)?, @"");

    Ok(())
}
