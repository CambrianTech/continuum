//! The v1 policy→mechanism control file — the Rust half of the
//! `GGML_MOE_PLAN_FILE` seam (#276, contract locked with BigMama on
//! #general 2026-08-01; her C++ `ResidencyCache` consumer landed as
//! k3-adopt f44ba7848).
//!
//! The Rust controller (TierPolicy / DecayBandit) emits exactly the
//! three actuator knobs the C++ residency mechanism accepts — a pin
//! list, the recency window length, and the host-cache byte budget —
//! as one small JSON document written ATOMICALLY (tmp + rename in the
//! same directory) so the per-token mtime poll on the C++ side never
//! observes a torn write. Field names are the CROSS-LANGUAGE wire
//! contract: her parser binds to them literally, so the tests pin them
//! literally.
//!
//! v1 is deliberately minimal ("the three knobs, nothing more"); per-
//! expert tier overrides join the document when multi-tier containers
//! ship. Transport upgrades (IPC instead of file) are v2 — the document
//! shape survives the transport.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Wire version — bump ONLY with a coordinated change on the C++ side.
pub const PLAN_FILE_VERSION: u32 = 1;

/// One pinned expert: (layer, expert) in the router's coordinate space
/// (matches `ExpertId`; tier implicit 0 in v1 single-tier containers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanPin {
    pub layer: u32,
    pub expert: u32,
}

/// The v1 control document — the three actuator knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanFileDocument {
    pub version: u32,
    /// Host-cache byte budget the mechanism enforces.
    pub budget_bytes: u64,
    /// Recency window length in tokens.
    pub window_k: u32,
    /// Experts to keep resident ALWAYS (the guaranteed-hit tier — the
    /// 184 shared experts first).
    pub pin_list: Vec<PlanPin>,
}

impl PlanFileDocument {
    pub fn new(budget_bytes: u64, window_k: u32, pin_list: Vec<PlanPin>) -> Self {
        Self {
            version: PLAN_FILE_VERSION,
            budget_bytes,
            window_k,
            pin_list,
        }
    }
}

/// Atomically publish `doc` at `path`: write to a sibling tmp file,
/// flush, then rename over the target. Rename-in-same-directory is the
/// atomicity the consumer's mtime poll relies on — it either sees the
/// old complete document or the new complete document, never a torn
/// one. Fails loud on any I/O error (a silently-unwritten plan is a
/// frozen actuator).
pub fn write_plan_file(path: &Path, doc: &PlanFileDocument) -> std::io::Result<()> {
    let json = serde_json::to_vec(doc).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&json)?;
        f.flush()?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches (#276 cross-language contract): the literal
    /// field names her C++ parser (k3-adopt f44ba7848) binds to. A
    /// serde rename would round-trip fine in Rust while silently
    /// freezing her actuator — this breaks first.
    #[test]
    fn wire_field_names_are_pinned_literally() {
        let doc = PlanFileDocument::new(
            42_949_672_960,
            24,
            vec![
                PlanPin {
                    layer: 0,
                    expert: 7,
                },
                PlanPin {
                    layer: 25,
                    expert: 183,
                },
            ],
        );
        let json = serde_json::to_string(&doc).expect("serialize");
        for key in [
            "\"version\":1",
            "\"budget_bytes\":42949672960",
            "\"window_k\":24",
            "\"pin_list\":[",
            "{\"layer\":0,\"expert\":7}",
            "{\"layer\":25,\"expert\":183}",
        ] {
            assert!(json.contains(key), "wire document missing {key}: {json}");
        }
        let back: PlanFileDocument = serde_json::from_str(&json).expect("round trip");
        assert_eq!(back, doc);
    }

    /// what this catches: the atomic publish — the target path always
    /// holds a COMPLETE, parseable document after every write, the tmp
    /// sibling never lingers, and a rewrite replaces content fully
    /// (her per-token mtime poll must never see a torn or stale-mixed
    /// plan).
    #[test]
    fn write_is_atomic_and_replaces_fully() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("moe-plan.json");

        let first = PlanFileDocument::new(
            1_000,
            8,
            vec![PlanPin {
                layer: 1,
                expert: 2,
            }],
        );
        write_plan_file(&path, &first).expect("first write");
        let read1: PlanFileDocument =
            serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
        assert_eq!(read1, first);
        assert!(
            !path.with_extension("tmp").exists(),
            "tmp sibling must not linger after the rename"
        );

        let second = PlanFileDocument::new(
            2_000,
            16,
            (0..184)
                .map(|e| PlanPin {
                    layer: 0,
                    expert: e,
                })
                .collect(),
        );
        write_plan_file(&path, &second).expect("rewrite");
        let read2: PlanFileDocument =
            serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
        assert_eq!(read2, second, "rewrite must fully replace the document");
        assert_eq!(read2.pin_list.len(), 184);
    }
}
