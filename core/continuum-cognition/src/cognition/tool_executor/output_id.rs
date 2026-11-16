//! The id of one saved output: its content address, so one id names one content forever.
//!
//! An id is minted only from the content it names ([`OutputId::of`]) or parsed from her call
//! ([`OutputId::parse`]); there is no other constructor. What it names cannot change after it is
//! written: the spill's writer refuses to replace an existing address with different bytes
//! (`spill::write_in`), and nothing else writes the tool-output class. A later act never changes
//! what an existing id returns (IntelMac 0051cdcf, 2026-11-14: a reopened id seemed to grow with her
//! own later reads; each read was in fact a NEW id, minted for a receipt that saved her previous
//! read inside it, so every read nested one level deeper).

use continuum_context::cause::ContentHash;

/// One saved output's id: 16 hex digits of its content's SHA-256 (`ContentHash::address`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OutputId(String);

/// A string she named as an output id that cannot be one: the path guard as a type (no `/`, no
/// `..`, no absolute path survives the hex-only rule), so a resolve never touches a path outside
/// her own directory.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid output id '{given}': expected a hex output id")]
pub struct NotAnOutputId {
    given: String,
}

impl OutputId {
    /// The id `content` is saved under.
    pub fn of(content: &str) -> Self {
        Self(ContentHash::of(content).address())
    }

    /// An id as she named it: non-empty hex, else refused with what she gave.
    pub fn parse(given: &str) -> Result<Self, NotAnOutputId> {
        if !given.is_empty() && given.chars().all(|c| c.is_ascii_hexdigit()) {
            Ok(Self(given.to_owned()))
        } else {
            Err(NotAnOutputId { given: given.to_owned() })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for OutputId {
    type Error = NotAnOutputId;

    fn try_from(given: String) -> Result<Self, Self::Error> {
        Self::parse(&given)
    }
}

impl From<OutputId> for String {
    fn from(id: OutputId) -> Self {
        id.0
    }
}

impl std::fmt::Display for OutputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an id that names more than one content (rule 1 of card fc55ff7b): the id
    // is the content's address, so equal content shares one id and any change of content is a
    // different id; and a path-shaped id (`../x`, `a/b`, empty) never parses, on the wire or off it.
    #[test]
    fn an_id_is_its_content_s_address_and_only_hex_parses() {
        let first = OutputId::of("test it_works ... FAILED\n");
        assert_eq!(first, OutputId::of("test it_works ... FAILED\n"));
        assert_ne!(first, OutputId::of("test it_works ... FAILED\nread again\n"));
        assert_eq!(OutputId::parse(first.as_str()), Ok(first.clone()));
        for bad in ["", "../etc", "ab/cd", "h1", "deadbeef "] {
            assert!(OutputId::parse(bad).is_err(), "{bad:?} parsed");
            assert!(serde_json::from_value::<OutputId>(serde_json::json!(bad)).is_err(), "{bad:?} deserialized");
        }
        let Ok(wire) = serde_json::to_value(&first) else { panic!("an id serializes") };
        assert_eq!(wire, serde_json::json!(first.as_str()));
    }
}
