use std::io;

use anyhow::{Result, bail};
use axum::{
    RequestPartsExt,
    body::{Body, Bytes},
    extract::{
        FromRequest, FromRequestParts, OptionalFromRequest,
        OptionalFromRequestParts, Request, rejection::BytesRejection,
    },
    http::{
        HeaderMap, StatusCode,
        header::{self, ToStrError},
        request,
    },
    response::{IntoResponse, Response},
};
use cookie::{Cookie, CookieJar};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    ServerState, impl_deref, signing::SignedData, utils::response_text,
};

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
        let Some(cookie) = parts.headers.get(header::COOKIE) else {
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
    fn into_response(self) -> Response {
        tracing::info!("rejecting cookie: {self}");
        response_text(StatusCode::BAD_REQUEST, self.to_response_msg())
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
    fn into_response(self) -> Response {
        tracing::info!("rejecting token: {self}");
        response_text(
            StatusCode::UNAUTHORIZED,
            match self {
                Self::Cookie(reject) => reject.to_response_msg(),
                Self::DeserializeToken(_) => "invalid token encoding",
                Self::Signature(_) => "invalid token signature",
                Self::Missing => "missing token in cookie",
            },
        )
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Cbor<T>(pub T);

impl_deref!(impl<T> mut Cbor<T> => T = .0);

impl<T> Cbor<T> {
    fn check_content_type(headers: &HeaderMap) -> anyhow::Result<()> {
        let Some(content_type) = headers.get(header::CONTENT_TYPE) else {
            bail!("missing Content-Type header");
        };

        let Ok(content_type) = content_type.to_str() else {
            bail!("invalid value of Content-Type")
        };

        let Ok(mime) = content_type.parse::<mime::Mime>() else {
            bail!("invalid mime value");
        };

        let is_cbor = mime.type_() == "application"
            && (mime.subtype() == "cbor"
                || mime.suffix().is_some_and(|name| name == "cbor"));

        if !is_cbor {
            bail!("not cbor");
        }

        Ok(())
    }
}

impl<T, S> FromRequest<S> for Cbor<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = CborRejection;

    async fn from_request(
        req: Request,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Self::check_content_type(req.headers())
            .map_err(CborRejection::ContentType)?;

        let bytes = Bytes::from_request(req, state).await?;

        ciborium::from_reader::<T, _>(&bytes as &[u8])
            .map_err(CborRejection::Cbor)
            .map(Self)
    }
}

impl<T, S> OptionalFromRequest<S> for Cbor<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = CborRejection;

    async fn from_request(
        req: Request,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        let hdrs = req.headers();
        if hdrs.get(header::CONTENT_TYPE).is_none() {
            return Ok(None);
        }

        Self::check_content_type(hdrs)
            .map_err(CborRejection::ContentType)?;

        let bytes = Bytes::from_request(req, state).await?;

        ciborium::from_reader::<T, _>(&bytes as &[u8])
            .map_err(CborRejection::Cbor)
            .map(Self)
            .map(Some)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CborRejection {
    #[error("invalid Content-Type: {0:?}")]
    ContentType(anyhow::Error),
    #[error("failed to read body: {0}")]
    Bytes(#[from] BytesRejection),
    #[error("failed to parse body: {0}")]
    Cbor(#[from] ciborium::de::Error<std::io::Error>),
}

impl IntoResponse for CborRejection {
    fn into_response(self) -> Response {
        fn internal_server_error() -> Response<Body> {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap()
        }

        tracing::warn!("rejecting cbor: {self}");

        match self {
            Self::ContentType(_) => response_text(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "expect application/cbor",
            ),
            Self::Bytes(_) => internal_server_error(),
            Self::Cbor(err) => match err {
                ciborium::de::Error::Syntax(idx) => response_text(
                    StatusCode::BAD_REQUEST,
                    format!("idx: {idx}"),
                ),
                ciborium::de::Error::Semantic(idx, msg) => response_text(
                    StatusCode::BAD_REQUEST,
                    format!("{msg}: {idx:?}"),
                ),
                _ => internal_server_error(),
            },
        }
    }
}

impl<T: Serialize> IntoResponse for Cbor<T> {
    fn into_response(self) -> Response {
        let mut buf = Vec::<u8>::new();
        let result = ciborium::into_writer(&*self, &mut buf);

        match result {
            Ok(()) => ([(header::CONTENT_TYPE, "application/cbor")], buf)
                .into_response(),
            Err(err) => {
                tracing::error!(
                    "failed to serialize response data as cbor: {err}"
                );
                (StatusCode::INTERNAL_SERVER_ERROR, Body::empty())
                    .into_response()
            }
        }
    }
}

impl<T: Serialize> TryFrom<Cbor<T>> for Body {
    type Error = ciborium::ser::Error<io::Error>;

    fn try_from(value: Cbor<T>) -> Result<Self, Self::Error> {
        let mut buf = Vec::<u8>::new();
        ciborium::into_writer(&*value, &mut buf)?;

        Ok(Self::from(buf))
    }
}
