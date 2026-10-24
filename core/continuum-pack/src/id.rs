//! `ContentId`: the one identity of every blob and manifest. Full SHA-256 on the stack; the
//! `sha256:<hex>` spelling exists only at the edges (Display, FromStr, serde).

use std::fmt;
use std::io::{self, Write};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

/// The algorithm prefix of the rendered form; the only algorithm this layer names.
const PREFIX: &str = "sha256:";

/// A blob's or a manifest's address: the SHA-256 of its bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentId([u8; 32]);

impl ContentId {
    /// An id from raw digest bytes.
    pub const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    /// The raw digest.
    pub const fn digest(&self) -> &[u8; 32] {
        &self.0
    }

    /// The id of `bytes`, hashed in one pass.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    /// Lowercase hex of the digest, without the prefix (a file name in a store).
    pub fn hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(64);
        for b in self.0 {
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0x0f)]));
        }
        out
    }
}

impl fmt::Display for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PREFIX}{}", self.hex())
    }
}

impl fmt::Debug for ContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Why a string is not a `ContentId`. One spelling only: lowercase hex after `sha256:`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("content id must start with `sha256:`")]
    Prefix,
    #[error("content id digest must be 64 hex characters, found {0}")]
    Length(usize),
    #[error("content id digest must be lowercase hex")]
    Hex,
}

impl FromStr for ContentId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix(PREFIX).ok_or(IdError::Prefix)?;
        if hex.len() != 64 {
            return Err(IdError::Length(hex.len()));
        }
        let nibble = |c: u8| match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            _ => Err(IdError::Hex),
        };
        let mut out = [0u8; 32];
        for (slot, pair) in out.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
            *slot = (nibble(pair[0])? << 4) | nibble(pair[1])?;
        }
        Ok(Self(out))
    }
}

impl Serialize for ContentId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ContentId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A writer that hashes every byte on its way through: a blob is hashed in flight, never
/// read back to be hashed.
pub(crate) struct HashingWriter<W> {
    inner: W,
    hasher: Sha256,
    written: u64,
}

impl<W: Write> HashingWriter<W> {
    pub(crate) fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            written: 0,
        }
    }

    /// The id and length of everything written, and the inner writer.
    pub(crate) fn finish(self) -> (ContentId, u64, W) {
        (
            ContentId(self.hasher.finalize().into()),
            self.written,
            self.inner,
        )
    }
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the edge spelling round-trips exactly, and every malformed spelling
    // (no prefix, short, uppercase) is refused rather than read as some other id.
    #[test]
    fn the_rendered_id_round_trips_and_only_one_spelling_parses() {
        let id = ContentId::of(b"abc");
        let s = id.to_string();
        assert_eq!(
            s,
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(s.parse::<ContentId>(), Ok(id));
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{s}\""));
        assert_eq!(
            serde_json::from_str::<ContentId>(&format!("\"{s}\"")).unwrap(),
            id
        );
        assert_eq!(s[7..].parse::<ContentId>(), Err(IdError::Prefix));
        assert_eq!("sha256:abc".parse::<ContentId>(), Err(IdError::Length(3)));
        assert_eq!(
            s.to_uppercase()
                .replace("SHA256", "sha256")
                .parse::<ContentId>(),
            Err(IdError::Hex)
        );
    }

    // what this catches: the in-flight hash equals the one-shot hash, so a streamed blob and a
    // buffered blob of the same bytes get the same id.
    #[test]
    fn hashing_in_flight_matches_hashing_at_once() {
        let data = vec![7u8; 100_003];
        let mut w = HashingWriter::new(Vec::new());
        for chunk in data.chunks(4096) {
            w.write_all(chunk).unwrap();
        }
        let (id, n, out) = w.finish();
        assert_eq!((id, n, out.len()), (ContentId::of(&data), 100_003, 100_003));
    }
}
