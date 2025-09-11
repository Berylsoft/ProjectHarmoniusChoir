use std::{fmt::Debug, pin::Pin, time::Duration};

use anyhow::Context;
use chrono::{DateTime, Datelike, FixedOffset, TimeDelta, Timelike, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

fn to_wechat_datetime(dt: DateTime<Utc>) -> Box<str> {
    let offset = FixedOffset::east_opt(8 * 3600)
        .expect("FixedOffset::east_opt(8 * 3600)");
    let dt = dt.with_timezone(&offset);
    let year = dt.year();
    let month = dt.month();
    let day = dt.day();
    let hour = dt.hour();
    let minute = dt.minute();
    format!("{year}年{month}月{day}日 {hour:0>2}:{minute:0>2}").into()
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Unknown(#[from] anyhow::Error),
    #[error("invalid js_code: {0}")]
    InvalidJscode(Box<str>),
    #[error("rate limited: {0}")]
    RateLimited(Box<str>),
    #[error("high risk: {0}")]
    HighRisk(Box<str>),
    #[error("system error: {0}")]
    System(Box<str>),
    #[error("invalid credential: {0}")]
    InvalidCredential(Box<str>),
    #[error("invalid openid: {0}")]
    InvalidOpenId(Box<str>),
    #[error("invalid access_token: {0}")]
    InvalidAccessToken(Box<str>),
    #[error("invalid template_id: {0}")]
    InvalidTemplateId(Box<str>),
    #[error("user not subscribe: {0}")]
    UserNotSubscribe(Box<str>),
    #[error("subscribe banned: {0}")]
    SubscribeBanned(Box<str>),
    #[error("too much message to one user: {0}")]
    TooMuchMessageToOneUser(Box<str>),
    #[error("keyword banned: {0}")]
    KeywordBanned(Box<str>),
    #[error("parameter error: {0}")]
    ParameterError(Box<str>),
}

pub type Result<T> = core::result::Result<T, Error>;

pub trait Wechat: Debug + Sync + Send {
    /// # Returns
    /// `openid`
    fn jscode2session<'a, 'fut>(
        &'a self,
        js_code: Box<str>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<str>>> + Send + 'fut>>
    where
        'a: 'fut;

    fn send_message<'a, 'fut>(
        &'a self,
        page: Box<str>,
        touser: Box<str>,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'fut>>
    where
        'a: 'fut;
}

#[derive(Debug)]
pub struct Message {
    content: Box<str>,
    result: Box<str>,
    time: DateTime<Utc>,
}

pub(crate) struct WechatImpl {
    app_id: Box<str>,
    app_secret: Box<str>,
    template_id: Box<str>,
    wechat_channel: Box<str>,

    http_client: reqwest::Client,
    access_token: Mutex<AccessToken>,
}

struct AccessToken {
    token: Box<str>,
    expire_at: DateTime<Utc>,
}

impl WechatImpl {
    pub(crate) fn new(
        app_id: Box<str>,
        app_secret: Box<str>,
        template_id: Box<str>,
        wechat_channel: Box<str>,
    ) -> Self {
        let http_client = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest::ClientBuilder::build()");
        let access_token = Mutex::new(AccessToken {
            token: "".into(),
            expire_at: DateTime::from_timestamp(0, 0)
                .expect("DateTime::from_timestamp(0, 0)"),
        });
        Self {
            app_id,
            app_secret,
            template_id,
            wechat_channel,

            http_client,
            access_token,
        }
    }
}

impl Debug for WechatImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WechatImpl").finish_non_exhaustive()
    }
}

impl Wechat for WechatImpl {
    fn jscode2session<'a, 'fut>(
        &'a self,
        js_code: Box<str>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<str>>> + Send + 'fut>>
    where
        'a: 'fut,
    {
        let fut = async move {
            #[derive(Deserialize)]
            struct Jscode2SessionResult {
                openid: Box<str>,
                errcode: i32,
                errmsg: Box<str>,
            }
            let res = self
                .http_client
                .get("https://api.weixin.qq.com/sns/jscode2session")
                .query(&[
                    ("appid", self.app_id.as_ref()),
                    ("secret", self.app_secret.as_ref()),
                    ("js_code", js_code.as_ref()),
                    ("grant_type", "authorization_code"),
                ])
                .send()
                .await
                .context("jscode2session req")?;
            let Jscode2SessionResult {
                openid,
                errcode,
                errmsg,
            }: Jscode2SessionResult =
                res.json().await.context("jscode2session res.json")?;
            match errcode {
                0 => Ok(openid),
                40029 => Err(Error::InvalidJscode(errmsg)),
                45011 => Err(Error::RateLimited(errmsg)),
                40226 => Err(Error::HighRisk(errmsg)),
                -1 => Err(Error::System(errmsg)),
                errcode => Err(Error::Unknown(anyhow::anyhow!(
                    "{errcode}: {errmsg}"
                ))),
            }
        };

        Box::pin(fut)
    }

    fn send_message<'a, 'fut>(
        &'a self,
        page: Box<str>,
        touser: Box<str>,
        message: Message,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'fut>>
    where
        'a: 'fut,
    {
        let fut = async move {
            #[derive(Serialize)]
            struct SendMessageRequest<'a> {
                touser: Box<str>,
                template_id: &'a str,
                page: Box<str>,
                miniprogram_state: &'a str,
                lang: &'static str,
                data: Data,
            }
            #[derive(Serialize)]
            struct Data {
                #[serde(rename = "thing2")]
                content: Value<Box<str>>,
                #[serde(rename = "phrase1")]
                result: Value<Box<str>>,
                #[serde(rename = "date3")]
                time: Value<Box<str>>,
            }
            #[derive(Serialize)]
            struct Value<T> {
                value: T,
            }
            #[derive(Deserialize)]
            struct SendMessageResult {
                errcode: i32,
                errmsg: Box<str>,
            }
            let Message {
                content,
                result,
                time,
            } = message;
            let time = to_wechat_datetime(time);
            let data = Data {
                content: Value { value: content },
                result: Value { value: result },
                time: Value { value: time },
            };
            let access_token = self.get_access_token().await?;
            let body = SendMessageRequest {
                touser,
                template_id: &self.template_id,
                page,
                miniprogram_state: &self.wechat_channel,
                lang: "zh_CN",
                data,
            };
            let res = self
                .http_client
                .post("https://api.weixin.qq.com/cgi-bin/message/subscribe/send")
                .query(&[
                    ("access_token", access_token.as_ref()),
                ])
                .json(&body)
                .send()
                .await
                .context("send_message req")?;
            let SendMessageResult { errcode, errmsg }: SendMessageResult =
                res.json().await.context("send_message res.json")?;
            match errcode {
                0 => Ok(()),
                40001 => Err(Error::InvalidCredential(errmsg)),
                40003 => Err(Error::InvalidOpenId(errmsg)),
                40014 => Err(Error::InvalidAccessToken(errmsg)),
                40037 => Err(Error::InvalidTemplateId(errmsg)),
                43101 => Err(Error::UserNotSubscribe(errmsg)),
                43107 => Err(Error::SubscribeBanned(errmsg)),
                43108 => Err(Error::TooMuchMessageToOneUser(errmsg)),
                45168 => Err(Error::KeywordBanned(errmsg)),
                47003 => Err(Error::ParameterError(errmsg)),
                -1 => Err(Error::System(errmsg)),
                errcode => Err(Error::Unknown(anyhow::anyhow!(
                    "{errcode}: {errmsg}"
                ))),
            }
        };

        Box::pin(fut)
    }
}

impl WechatImpl {
    async fn get_access_token(&self) -> Result<Box<str>> {
        let api_get_access_token = async || -> Result<_> {
            #[derive(Deserialize)]
            struct GetAccessTokenResult {
                access_token: Box<str>,
                expires_in: i64,
            }
            let res = self
                .http_client
                .get("https://api.weixin.qq.com/cgi-bin/token")
                .query(&[
                    ("appid", self.app_id.as_ref()),
                    ("secret", self.app_secret.as_ref()),
                    ("grant_type", "client_credential"),
                ])
                .send()
                .await
                .context("get_access_token req")?;

            let GetAccessTokenResult {
                access_token,
                expires_in,
            } = res.json().await.context("get_access_token res.json")?;

            let access_token = AccessToken {
                token: access_token,
                expire_at: Utc::now() + TimeDelta::seconds(expires_in),
            };

            Ok(access_token)
        };

        let mut access_token = self.access_token.lock().await;

        if access_token.expire_at <= Utc::now() {
            let new_access_token = api_get_access_token().await?;
            if new_access_token.expire_at <= Utc::now() {
                return Err(anyhow::anyhow!(
                    "unexpected immediate expire"
                )
                .into());
            }
            *access_token = new_access_token;
        }

        Ok(access_token.token.clone())
    }
}
