use anyhow::Context;
use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use ed25519_dalek::{
    SIGNATURE_LENGTH, Signature, SigningKey, VerifyingKey,
    ed25519::signature::Signer,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::utils::to_cbor;

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedData<T> {
    data: T,
    #[serde(with = "serde_bytes")]
    signature: [u8; SIGNATURE_LENGTH],
}

impl<T: Serialize + DeserializeOwned> SignedData<T> {
    /// # Panics
    /// if [`T`] failed to serialize
    pub fn to_encoded(&self) -> Box<str> {
        BASE64_URL_SAFE_NO_PAD
            .encode(to_cbor(&self).unwrap())
            .into()
    }

    /// # Errors
    /// not valid base64 of url safe no pad
    /// unable to encoded data into [`SignedData<T>`]
    pub fn try_from_encoded(
        encoded: impl AsRef<[u8]>,
    ) -> anyhow::Result<Self> {
        let buf = BASE64_URL_SAFE_NO_PAD
            .decode(encoded)
            .context("invalid base64")?;

        ciborium::from_reader::<Self, _>(&buf as &[u8])
            .context("invalid data")
    }

    /// # Panics
    /// if [`T`] failed to serialize
    pub fn sign(data: T, key: &SigningKey) -> Self {
        let signature = key.sign(&to_cbor(&data).unwrap()).to_bytes();
        Self { data, signature }
    }

    /// # Errors
    /// if invalid signature
    /// # Panics
    /// if [`T`] failed to serialize
    pub fn verify(self, key: &VerifyingKey) -> anyhow::Result<T> {
        key.verify_strict(
            &to_cbor(&self.data).unwrap(),
            &Signature::from_bytes(&self.signature),
        )
        .context("invalid signature")
        .map(|()| self.data)
    }
}

pub trait IntoSigned {
    fn into_signed(self, key: &SigningKey) -> SignedData<Self>
    where
        Self: Sized;
}

impl<T> IntoSigned for T
where
    T: Serialize + DeserializeOwned,
{
    fn into_signed(self, key: &SigningKey) -> SignedData<Self>
    where
        Self: Sized,
    {
        SignedData::sign(self, key)
    }
}
