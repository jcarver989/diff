use crate::FingerprintError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fingerprint([u8; 32]);

impl Fingerprint {
    #[must_use]
    pub fn of<I, T>(fields: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: AsRef<[u8]>,
    {
        let mut hasher = blake3::Hasher::new();
        for field in fields {
            let field = field.as_ref();
            hasher.update(&u64::try_from(field.len()).unwrap_or(u64::MAX).to_le_bytes());
            hasher.update(field);
        }
        Self(*hasher.finalize().as_bytes())
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub const fn to_u64(self) -> u64 {
        u64::from_le_bytes([
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5], self.0[6], self.0[7],
        ])
    }

    #[must_use]
    pub fn to_hex(self) -> String {
        let mut hex = String::with_capacity(64);
        for byte in self.0 {
            hex.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            hex.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        hex
    }

    pub fn from_hex(text: &str) -> Result<Self, FingerprintError> {
        let bytes = text.as_bytes();
        if bytes.len() != 64 {
            return Err(FingerprintError::Length(bytes.len()));
        }
        let mut digest = [0_u8; 32];
        let (pairs, _) = bytes.as_chunks::<2>();
        for (slot, pair) in digest.iter_mut().zip(pairs) {
            *slot = (nibble(pair[0])? << 4) | nibble(pair[1])?;
        }
        Ok(Self(digest))
    }
}

fn nibble(byte: u8) -> Result<u8, FingerprintError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(FingerprintError::Digit(char::from(byte))),
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fingerprint({})", self.to_hex())
    }
}

impl Serialize for Fingerprint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Fingerprint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_hex(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_fields_so_concatenations_do_not_collide() {
        assert_ne!(Fingerprint::of(["ab", "c"]), Fingerprint::of(["a", "bc"]));
        assert_ne!(Fingerprint::of(["a\0", "b"]), Fingerprint::of(["a", "\0b"]));
        assert_eq!(Fingerprint::of(["a", "b"]), Fingerprint::of(["a", "b"]));
    }

    #[test]
    fn round_trips_through_hexadecimal() {
        let fingerprint = Fingerprint::of(["value"]);
        let hex = fingerprint.to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(Fingerprint::from_hex(&hex).unwrap(), fingerprint);
        assert!(Fingerprint::from_hex("beef").is_err());
        assert!(Fingerprint::from_hex(&"z".repeat(64)).is_err());
    }

    #[test]
    fn serializes_as_a_hexadecimal_string() {
        let fingerprint = Fingerprint::of(["value"]);
        let json = serde_json::to_string(&fingerprint).unwrap();
        assert_eq!(json, format!("\"{}\"", fingerprint.to_hex()));
        assert_eq!(
            serde_json::from_str::<Fingerprint>(&json).unwrap(),
            fingerprint
        );
    }
}
