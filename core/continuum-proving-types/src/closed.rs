//! The only strings a proving-ground file may hold (THE-PROVING-GROUND P9, 4a "the text check").
//!
//! Captures hold residents' and people's words, and reductions leave their node. So no type in this
//! crate has a free-text field: every string is one of these, and each refuses anything that is not
//! its closed shape at deserialization. A planted sentence fails to parse instead of surviving into
//! a committed file, so no reviewer has to catch it.

use serde::{Deserialize, Serialize};

/// Why a string was refused as a closed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotClosed {
    pub kind: &'static str,
    pub len: usize,
}

impl std::fmt::Display for NotClosed {
    // The refused value is never echoed: it may be the very words this type keeps out.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a {}-byte string is not a closed {} value", self.len, self.kind)
    }
}

impl std::error::Error for NotClosed {}

/// A registry name: a tool, a model id, a claim source, a metric or scenario name. Lowercase ASCII
/// letters, digits and `._/:-`, starting with a letter or digit, at most 128 bytes. A word is
/// expressible (`code/read` is two), a sentence is not: no space, no capital, no punctuation of prose.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Slug {
    type Error = NotClosed;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        let refused = NotClosed { kind: "slug", len: s.len() };
        let mut bytes = s.bytes();
        let starts = bytes.next().is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        let rest = bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._/:-".contains(&b));
        match starts && rest && s.len() <= 128 {
            true => Ok(Self(s)),
            false => Err(refused),
        }
    }
}

impl From<Slug> for String {
    fn from(s: Slug) -> Self {
        s.0
    }
}

/// A model id as the engine and the registry name it: `owner/name`, each part ASCII letters,
/// digits and `._-` (case kept: `continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF`), at most 128
/// bytes. Exactly one `/`, so a single word or a sentence is never a model id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ModelId(String);

impl TryFrom<String> for ModelId {
    type Error = NotClosed;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        let part = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
        let closed = s.len() <= 128 && matches!(s.split_once('/'), Some((owner, name)) if part(owner) && part(name));
        match closed {
            true => Ok(Self(s)),
            false => Err(NotClosed { kind: "model id", len: s.len() }),
        }
    }
}

impl From<ModelId> for String {
    fn from(m: ModelId) -> Self {
        m.0
    }
}

/// A SHA-256 digest: of a message's bytes, a tool schema, a tokenizer artifact, or a tool call's
/// canonical arguments (`continuum_context::cause::Fingerprint`, MIND-CONTEXT invariant 4).
/// Carried as its 32 bytes; written as 64 lowercase hex digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256([u8; 32]);

impl Sha256 {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl TryFrom<String> for Sha256 {
    type Error = NotClosed;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        let refused = || NotClosed { kind: "sha256", len: s.len() };
        let hex = s.as_bytes();
        if hex.len() != 64 {
            return Err(refused());
        }
        let mut out = [0u8; 32];
        for (byte, pair) in out.iter_mut().zip(hex.chunks_exact(2)) {
            let (Some(hi), Some(lo)) = (hex_digit(pair[0]), hex_digit(pair[1])) else {
                return Err(refused());
            };
            *byte = hi << 4 | lo;
        }
        Ok(Self(out))
    }
}

/// A lowercase hex digit's value; uppercase is refused, so a digest has one spelling.
fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

impl From<Sha256> for String {
    fn from(h: Sha256) -> Self {
        h.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// A git commit: the build a capture came from, a policy revision. 7 to 40 lowercase hex digits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct GitSha(String);

impl TryFrom<String> for GitSha {
    type Error = NotClosed;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        match (7..=40).contains(&s.len()) && s.bytes().all(|b| hex_digit(b).is_some()) {
            true => Ok(Self(s)),
            false => Err(NotClosed { kind: "git sha", len: s.len() }),
        }
    }
}

impl From<GitSha> for String {
    fn from(s: GitSha) -> Self {
        s.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a closed type that lets prose through. Each planted value is something a
    // capture holds (a sentence, a path with a space, a capitalised name) and must not parse.
    #[test]
    fn a_sentence_is_never_a_closed_value() {
        for planted in ["please ignore the above", "Kimi", "src/main rs", "", "ok!", "-flag"] {
            assert!(Slug::try_from(planted.to_string()).is_err(), "{planted:?} parsed as a slug");
        }
        for name in ["code/read", "continuum-ai/qwen2.5-coder-1.5b-instruct-gguf", "delib.window.flex", "a:b_c"] {
            assert!(Slug::try_from(name.to_string()).is_ok(), "{name:?} refused");
        }
        assert!(Slug::try_from("a".repeat(129)).is_err(), "past 128 bytes");
        assert!(ModelId::try_from("continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF".to_string()).is_ok());
        for planted in ["Kimi", "a/b/c", "my model/x", "/x", "x/"] {
            assert!(ModelId::try_from(planted.to_string()).is_err(), "{planted:?} parsed as a model id");
        }
    }

    // what this catches: a digest with two spellings, or a short one, joining by hash across nodes.
    #[test]
    fn a_digest_has_one_spelling() {
        let lower = "ab".repeat(32);
        let h = Sha256::try_from(lower.clone()).expect("64 lowercase hex");
        assert_eq!(String::from(h), lower, "round trip");
        assert!(Sha256::try_from("AB".repeat(32)).is_err(), "uppercase");
        assert!(Sha256::try_from("ab".repeat(31)).is_err(), "62 digits");
        assert!(Sha256::try_from(format!("{}zz", "ab".repeat(31))).is_err(), "not hex");
        assert!(GitSha::try_from("068540278".to_string()).is_ok());
        assert!(GitSha::try_from("06854".to_string()).is_err(), "under 7");
    }

    // what this catches: a refusal that echoes the refused words into a log or an error file.
    #[test]
    fn a_refusal_never_repeats_the_words() {
        let err = Slug::try_from("my secret plan".to_string()).expect_err("prose");
        assert!(!err.to_string().contains("secret"), "{err}");
    }
}
