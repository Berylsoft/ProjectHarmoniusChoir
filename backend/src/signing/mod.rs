use std::io::Cursor;

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
    pub fn to_encoded(&self) -> Box<str> {
        BASE64_URL_SAFE_NO_PAD
            .encode(to_cbor(&self).unwrap())
            .into()
    }

    pub fn try_from_encoded(
        encoded: impl AsRef<[u8]>,
    ) -> anyhow::Result<Self> {
        let buf = BASE64_URL_SAFE_NO_PAD
            .decode(encoded)
            .context("invalid base64")?;

        ciborium::from_reader::<SignedData<T>, _>(Cursor::new(buf))
            .context("invalid data")
    }

    pub fn sign(data: T, key: &SigningKey) -> SignedData<T> {
        let signature = key.sign(&to_cbor(&data).unwrap()).to_bytes();
        Self { data, signature }
    }

    pub fn verify(self, key: &VerifyingKey) -> anyhow::Result<T> {
        key.verify_strict(
            &to_cbor(&self.data).unwrap(),
            &Signature::from_bytes(&self.signature),
        )
        .context("invalid signature")
        .map(|()| self.data)
    }
}
