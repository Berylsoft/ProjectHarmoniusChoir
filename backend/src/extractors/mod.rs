use anyhow::Result;
use axum::{
    RequestPartsExt,
    body::Body,
    extract::{FromRequestParts, OptionalFromRequestParts},
    http::{Response, StatusCode, header::ToStrError, request},
    response::IntoResponse,
};
use cookie::{Cookie, CookieJar};
use serde::{Serialize, de::DeserializeOwned};

use crate::{ServerState, impl_deref, signing::SignedData};

#[derive(Debug, Default)]
pub struct Cookies(pub CookieJar);

impl_deref!(mut Cookies => CookieJar = .0);

impl<S> OptionalFromRequestParts<S> for Cookies
where
    S: Send + Sync,
{
    type Rejection = CookiesRejection;

    async fn from_request_parts(
        parts: &mut request::Parts,
        _: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        let Some(cookie) = parts.headers.get("Cookie") else {
            return Ok(None);
        };

        let cookie =
            cookie.to_str().map_err(CookiesRejection::HeaderValue)?;

        let cookie = Cookie::split_parse_encoded(cookie)
            .try_fold(CookieJar::new(), |mut jar, it| {
                jar.add_original(it?.into_owned());

                Result::<_, cookie::ParseError>::Ok(jar)
            })
            .map_err(CookiesRejection::CookieParse)?;

        Ok(Some(Self(cookie)))
    }
}

impl<S> FromRequestParts<S> for Cookies
where
    S: Send + Sync,
{
    type Rejection = CookiesRejection;

    async fn from_request_parts(
        parts: &mut request::Parts,
        _: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extract::<Option<Self>>()
            .await?
            .ok_or(CookiesRejection::Missing)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CookiesRejection {
    #[error("non ascii header value: {0}")]
    HeaderValue(ToStrError),
    #[error("invalid cookie: {0}")]
    CookieParse(cookie::ParseError),
    #[error("missing cookie header")]
    Missing,
}

impl CookiesRejection {
    pub const fn to_response_msg(&self) -> &'static str {
        match self {
            Self::HeaderValue(_) => "invalid cookie header value",
            Self::CookieParse(_) => "invalid cookie",
            Self::Missing => "missing cookie header cookie",
        }
    }
}

impl IntoResponse for CookiesRejection {
    fn into_response(self) -> axum::response::Response {
        tracing::info!("rejecting cookie: {self}");
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(self.to_response_msg()))
            .unwrap()
    }
}

#[derive(Debug)]
pub struct Token<T>(pub T);

impl_deref!(impl<T> ref Token<T> => T = .0);

impl<T: Serialize + DeserializeOwned> FromRequestParts<ServerState>
    for Token<T>
{
    type Rejection = TokenRejection;

    async fn from_request_parts(
        parts: &mut request::Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        let cookie = parts
            .extract::<Cookies>()
            .await
            .map_err(TokenRejection::Cookie)?;

        let token = cookie.get("token").ok_or(TokenRejection::Missing)?;

        let token = SignedData::<T>::try_from_encoded(token.value())
            .map_err(TokenRejection::DeserializeToken)?;

        let token = token
            .verify(&state.key.verifying_key())
            .map_err(TokenRejection::Signature)?;

        Ok(Self(token))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenRejection {
    #[error("cookie rejection: {0}")]
    Cookie(CookiesRejection),
    #[error("invalid token data: {0:?}")]
    DeserializeToken(anyhow::Error),
    #[error("invalid token signature: {0:?}")]
    Signature(anyhow::Error),
    #[error("missing token in cookie")]
    Missing,
}

impl IntoResponse for TokenRejection {
    fn into_response(self) -> axum::response::Response {
        tracing::info!("rejecting token: {self}");
        Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(match self {
                Self::Cookie(reject) => reject.to_response_msg(),
                Self::DeserializeToken(_) => "invalid token encoding",
                Self::Signature(_) => "invalid token signature",
                Self::Missing => "missing token in cookie",
            }))
            .unwrap()
    }
}
