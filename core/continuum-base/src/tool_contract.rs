//! A tool's contract identity: what a mind was shown about a verb, and which revision of its
//! meaning that was. Declared by the command itself (`CommandSpec::CONTRACT_REVISION` in the
//! kernel) beside its effect and disclosure, and read by the surface that freezes her hands for an
//! epoch (`continuum-cognition`'s `tools_custody`). It lives on the floor because the kernel
//! declares it and cognition reads it, and neither may import the other.
//!
//! Two parts, because they change for different reasons (card 9cf06ade, Codex's review 2026-11-17):
//! - every field of the spec she was sent (a schema or description edit changes them);
//! - the declared [`ContractRevision`] (a change in what a call DOES under the same schema changes
//!   only this, so the command must bump it).
//!
//! [`ContractId`] binds both, so neither change can pass for the other.

use sha2::{Digest, Sha256};

/// Which revision of a command's meaning its schema stands for. A command bumps it when a call
/// valid under the same schema now does something else; a schema or description edit needs no
/// bump (the wire bytes already differ). A command that declares nothing is [`Self::FIRST`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContractRevision(pub u32);

impl ContractRevision {
    pub const FIRST: Self = Self(1);
}

impl Default for ContractRevision {
    fn default() -> Self {
        Self::FIRST
    }
}

/// The identity of one verb as a mind was shown it: SHA-256 over its declared revision and every
/// field of its native spec as sent. Computed once where a surface is built, never per act.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContractId(#[serde(with = "hex_bytes")] pub [u8; 32]);

impl ContractId {
    /// `wire` is the verb's spec as it is sent, every field, so the identity changes exactly when
    /// what she reads changes.
    pub fn of(revision: ContractRevision, wire: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(revision.0.to_be_bytes());
        hasher.update(wire);
        Self(hasher.finalize().into())
    }

    /// The identity in hex: what a notice, a probe and a notice's key show.
    pub fn hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

mod hex_bytes {
    pub fn serialize<S: serde::Serializer>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
        let text: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        serializer.serialize_str(&text)
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<[u8; 32], D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        let mut out = [0u8; 32];
        if text.len() != 64 {
            return Err(serde::de::Error::invalid_length(text.len(), &"64 hex digits"));
        }
        for (slot, pair) in out.iter_mut().zip(text.as_bytes().chunks(2)) {
            let digits = std::str::from_utf8(pair).map_err(serde::de::Error::custom)?;
            *slot = u8::from_str_radix(digits, 16).map_err(serde::de::Error::custom)?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a same-schema meaning change passing as unchanged (Codex's review of
    // 9cf06ade): the revision alone must change the identity, and so must the bytes alone.
    #[test]
    fn revision_and_bytes_each_change_the_identity() {
        let base = ContractId::of(ContractRevision::FIRST, b"{\"name\":\"work_note\"}");
        assert_ne!(base, ContractId::of(ContractRevision(2), b"{\"name\":\"work_note\"}"));
        assert_ne!(base, ContractId::of(ContractRevision::FIRST, b"{\"name\":\"work_note\",\"x\":1}"));
        assert_eq!(base, ContractId::of(ContractRevision::FIRST, b"{\"name\":\"work_note\"}"));
    }

    // what this catches: a persisted identity that restores as a different one (it is compared
    // against the live surface after a restart).
    #[test]
    fn identity_round_trips_through_its_persisted_form() {
        let id = ContractId::of(ContractRevision::FIRST, b"spec");
        let text = serde_json::to_string(&id).expect("an id encodes");
        assert_eq!(serde_json::from_str::<ContractId>(&text).expect("its own encoding decodes"), id);
        assert!(serde_json::from_str::<ContractId>("\"abc\"").is_err(), "a short id is refused, never padded");
    }
}
