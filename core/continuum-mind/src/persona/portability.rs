//! Persona-home portability — package a persona's home (its identity + memory)
//! so the SAME individual can be re-spawned on another node. "Move the home,
//! keep the self."
//!
//! A persona's identity IS its memory: its home
//! (`personas/<name>/{airc/keypair, engrams.sqlite, seed.json}`) holds the airc
//! keypair (the identity token), the engram memory, and the seed. Bundling that
//! home and restoring it on another node — then `Airc::attach_as` the same
//! keypair — yields the SAME peer identity with the SAME engrams (proven locally
//! by `engram_context_survives_home_reopen_portability_proof`). This module is
//! the pack/unpack CORE only.
//!
//! ## Scope boundary (the joint pieces live elsewhere)
//! - **Transport is a separate concern.** The bundle carries a PRIVATE KEYPAIR,
//!   so it must ride ENCRYPTED across the grid — wrap [`PersonaHomeBundle::to_bytes`]
//!   in the airc stream-plane AEAD before it leaves the node. The bundle is
//!   plaintext at rest because it simply *is* the home on disk; sealing happens
//!   at the send boundary.
//! - **Cross-node trust is airc's job.** Once restored, node B `attach_as` the
//!   same keypair → same `peer_id`; the mesh recognising it as the same citizen
//!   across nodes (same key → signatures verify) is the airc §1/trust piece.
//! - **One-instance lease** (no split-brain — the same identity must not run
//!   live on two nodes at once) is a later, separate concern.
//!
//! See docs/architecture/IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md Part A
//! (identity = portable token bound to memory).

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::home::PersonaHome;

/// A portable snapshot of a persona's home: every file under the home root,
/// keyed by its path RELATIVE to the root, value base64-encoded so binary files
/// (the keypair, the SQLite engram db) ride intact through a text envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaHomeBundle {
    /// relative-path (forward-slashed) -> base64(file bytes)
    files: BTreeMap<String, String>,
}

impl PersonaHomeBundle {
    /// Pack a persona's home into a portable bundle, recursively reading every
    /// file under the home root. Contains the private keypair — SEAL before
    /// sending over the wire (see module docs).
    pub fn export(home: &PersonaHome) -> io::Result<Self> {
        let mut files = BTreeMap::new();
        collect_files(home.root(), home.root(), &mut files)?;
        Ok(Self { files })
    }

    /// Restore the bundle into a target home, writing every file under the
    /// target root (creating parent dirs). The target then carries the same
    /// identity keypair + engrams as the source — `attach_as` it to spawn the
    /// same individual on this node.
    pub fn restore(&self, target: &PersonaHome) -> io::Result<()> {
        let root = target.root();
        for (rel, b64) in &self.files {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let dest = root.join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(dest, bytes)?;
        }
        Ok(())
    }

    /// Serialize for transport. PLAINTEXT (contains the private keypair) — wrap
    /// in the stream-plane AEAD before it leaves the node.
    pub fn to_bytes(&self) -> io::Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Deserialize a bundle received from another node (after decryption).
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Number of files captured — smoke check / observability.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Recursively read every file under `dir`, storing each by its path relative
/// to `root` (forward-slashed for cross-platform restore).
fn collect_files(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = std::fs::read(&path)?;
            out.insert(rel, base64::engine::general_purpose::STANDARD.encode(bytes));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the portability CORE — packing a persona home and
    // restoring it on a fresh root reproduces every file byte-for-byte
    // (including the binary keypair + the SQLite engram db). This is the
    // continuum half of "move the home, keep the self": the same keypair (→
    // same identity) and the same engrams arrive intact on node B. Regression
    // here = a moved persona loses its identity or its memory in transit.
    #[test]
    fn export_then_restore_reproduces_home_byte_for_byte() {
        let src_dir = tempfile::tempdir().unwrap();
        let src = PersonaHome::from_root(src_dir.path().join("personas").join("ava"));
        src.ensure_exists().unwrap();
        // The three things that make a persona itself: identity token, memory,
        // seed. Binary keypair + sqlite are the important cases (base64 path).
        std::fs::create_dir_all(src.airc_dir()).unwrap();
        std::fs::write(src.airc_dir().join("identity.key"), [7u8, 0u8, 255u8, 42u8]).unwrap();
        std::fs::write(src.engrams_db(), b"\x00SQLite format 3\x00binary").unwrap();
        std::fs::write(src.seed_json(), br#"{"name":"ava"}"#).unwrap();

        // Pack -> wire bytes -> unpack (the full transport round-trip shape).
        let bundle = PersonaHomeBundle::export(&src).unwrap();
        assert_eq!(bundle.file_count(), 3, "captured keypair + engrams + seed");
        let wire = bundle.to_bytes().unwrap();
        let received = PersonaHomeBundle::from_bytes(&wire).unwrap();
        assert_eq!(received, bundle, "bundle survives the wire round-trip");

        // Restore onto a DIFFERENT root — "node B".
        let dst_dir = tempfile::tempdir().unwrap();
        let dst = PersonaHome::from_root(dst_dir.path().join("personas").join("ava"));
        received.restore(&dst).unwrap();

        // Every byte identical → same identity token, same memory.
        assert_eq!(
            std::fs::read(dst.airc_dir().join("identity.key")).unwrap(),
            vec![7u8, 0u8, 255u8, 42u8],
            "keypair (the identity token) arrives byte-identical"
        );
        assert_eq!(
            std::fs::read(dst.engrams_db()).unwrap(),
            b"\x00SQLite format 3\x00binary",
            "engram db (the memory) arrives byte-identical"
        );
        assert_eq!(
            std::fs::read(dst.seed_json()).unwrap(),
            br#"{"name":"ava"}"#
        );
    }

    // what this catches: restore creates nested parent dirs (the `airc/`
    // subdir) under a target root that doesn't exist yet — spawn-on-a-fresh-node
    // must not require the operator to pre-create the home tree.
    #[test]
    fn restore_creates_missing_target_tree() {
        let src_dir = tempfile::tempdir().unwrap();
        let src = PersonaHome::from_root(src_dir.path().join("personas").join("ivar"));
        src.ensure_exists().unwrap();
        std::fs::create_dir_all(src.airc_dir()).unwrap();
        std::fs::write(src.airc_dir().join("identity.key"), [1u8, 2u8, 3u8]).unwrap();

        let bundle = PersonaHomeBundle::export(&src).unwrap();
        let dst_dir = tempfile::tempdir().unwrap();
        // Deeply-nested, not-yet-existing target.
        let dst = PersonaHome::from_root(dst_dir.path().join("a").join("b").join("ivar"));
        bundle.restore(&dst).unwrap();
        assert!(dst.airc_dir().join("identity.key").exists());
    }
}
