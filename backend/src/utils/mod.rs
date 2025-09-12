use std::{fmt::Write as _, io};

use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use cookie::{Cookie, SameSite};
use ed25519_dalek::SigningKey;
use serde::{Serialize, de::DeserializeOwned};

use crate::signing::SignedData;

pub mod boxed_u8_arr_hex;

#[macro_export]
macro_rules! impl_deref {
    ($(impl<$($ge:ident),*>)? mut $src:ty => $dst:ty = $($tt:tt)*) => {
        impl_deref!($(impl<$($ge),*>)? ref $src => $dst = $($tt)*);

        impl$(<$($ge),*>)? ::core::ops::DerefMut for $src {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self$($tt)*
            }
        }
    };

    ($(impl<$($ge:ident),*>)? ref $src:ty => $dst:ty = $($tt:tt)*) => {
        impl$(<$($ge),*>)? ::core::ops::Deref for $src {
            type Target = $dst;

            fn deref(&self) -> &Self::Target {
                &self$($tt)*
            }
        }
    };
}

pub fn to_cbor<T: Serialize>(
    data: &T,
) -> Result<Box<[u8]>, ciborium::ser::Error<io::Error>> {
    let mut buf = vec![];
    ciborium::into_writer(data, &mut buf)?;
    Ok(buf.into())
}

pub fn content_type_plain_text() -> (header::HeaderName, HeaderValue) {
    (
        header::CONTENT_TYPE,
        HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref()),
    )
}

pub fn response_text(
    status: impl Into<Option<StatusCode>>,
    text: impl Into<Body>,
) -> Response {
    let status: Option<StatusCode> = status.into();
    (
        status.unwrap_or(StatusCode::OK),
        [content_type_plain_text()],
        text.into(),
    )
        .into_response()
}

pub fn cookie_set_token<T>(
    token: T,
    key: &SigningKey,
) -> (HeaderName, HeaderValue)
where
    T: Serialize + DeserializeOwned,
{
    let mut c = Cookie::new(
        "token",
        SignedData::sign(token, key).to_encoded().to_string(),
    );
    c.set_max_age(cookie::time::Duration::days(365));
    c.set_secure(true);
    c.set_http_only(true);
    c.set_same_site(SameSite::Strict);

    (
        header::SET_COOKIE,
        HeaderValue::from_str(c.encoded().to_string().as_str()).unwrap(),
    )
}

pub fn rfc5987_utf8(data: impl AsRef<str>) -> Box<str> {
    let data = data.as_ref();
    let mut encoded = String::with_capacity(data.len());

    encoded.push_str("UTF-8''");

    let mut buf = [0_u8; 4];
    for ch in data.chars() {
        match ch {
            ch if ch.is_ascii_alphanumeric() => {
                encoded.push(ch);
            }
            '!' | '#' | '$' | '&' | '+' | '-' | '.' | '^' | '_' | '`'
            | '|' | '~' => {
                encoded.push(ch);
            }
            ch => {
                let utf8_bytes = ch.encode_utf8(&mut buf).as_bytes();
                for byte in utf8_bytes {
                    write!(&mut encoded, "%{byte:0>2x}")
                        .expect("no error when write to string");
                }
            }
        }
    }

    encoded.into_boxed_str()
}

#[inline]
pub fn length_check_quick(s: &str, limit: usize) -> bool {
    if s.len() > limit * 4 {
        return false;
    }

    let mut cnt = 0;
    for _ in s.chars() {
        cnt += 1;
        if cnt > limit {
            return false;
        }
    }

    true
}
