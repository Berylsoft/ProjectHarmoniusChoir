use std::io;

use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

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
    text: impl Into<Body>,
    status: impl Into<Option<StatusCode>>,
) -> Response {
    let status: Option<StatusCode> = status.into();
    (
        status.unwrap_or(StatusCode::OK),
        [content_type_plain_text()],
        text.into(),
    )
        .into_response()
}
