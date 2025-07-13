use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn deserialize<'de, D, const C: usize>(
    de: D,
) -> Result<Box<[u8; C]>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(de)?;
    let r = hex::decode(s).map_err(|e| {
        serde::de::Error::custom(format!("failed to decode hex {e}"))
    })?;

    if r.len() != C {
        return Err(serde::de::Error::custom(format!(
            "expect hex with data length of {C}, actual: {}",
            r.len()
        )));
    }

    let mut arr = Box::new([0_u8; C]);

    arr.copy_from_slice(&r);

    Ok(arr)
}

pub fn serialize<S, const C: usize>(
    v: &[u8; C],
    se: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let h = hex::encode(v.as_slice());
    h.serialize(se)
}
