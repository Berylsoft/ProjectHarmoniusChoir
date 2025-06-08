use std::io;

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
