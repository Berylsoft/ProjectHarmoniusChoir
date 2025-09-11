use std::{fmt::Debug, pin::Pin, time::Duration};

use anyhow::Context;
use serde::Deserialize;

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
}

pub type Result<T> = core::result::Result<T, Error>;

pub trait Wechat: Debug + Sync + Send {
    /// # Returns
    /// `openid`
    fn jscode2session<'a, 'fut>(
        &'a self,
        js_code: Box<str>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<str>>> + 'fut>>
    where
        'a: 'fut;
}

pub(crate) struct WechatImpl {
    http_client: reqwest::Client,
    app_id: Box<str>,
    app_secret: Box<str>,
}

impl WechatImpl {
    pub(crate) fn new(app_id: Box<str>, app_secret: Box<str>) -> Self {
        let http_client = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest::ClientBuilder::build()");
        Self {
            http_client,
            app_id,
            app_secret,
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
    ) -> Pin<Box<dyn Future<Output = Result<Box<str>>> + 'fut>>
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
}
