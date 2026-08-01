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
//! v1 stays minimal, and per-expert precision joined it as an OPTIONAL
//! field (Joel's WASTE-differentiator steer, #general 2026-08-01): a
//! pin may carry a `tier` — an index into the CONTAINER's declared
//! precision ladder — so hot/important experts are served at higher
//! fidelity while cold ones stream from the small-quant banks. Absent
//! tier = the v1 single-tier behavior, and serde skips `None`, so
//! tier-less plans are BYTE-IDENTICAL to the original wire: neither
//! side needs a lockstep upgrade. Transport upgrades (IPC instead of
//! file) are v2 — the document shape survives the transport.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Wire version — bump ONLY with a coordinated change on the C++ side.
pub const PLAN_FILE_VERSION: u32 = 1;

/// One pinned expert: (layer, expert) in the router's coordinate space
/// (matches `ExpertId`), optionally with a precision hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanPin {
    pub layer: u32,
    pub expert: u32,
    /// Precision hint: an INDEX into the container's declared precision
    /// ladder (manifest order, 0 = highest-fidelity bank). The policy
    /// layer never names quant formats — the container manifest owns the
    /// ladder; the plan only points into it. `None` = container-default
    /// precision (the v1 single-tier behavior); serde omits it entirely
    /// so tier-less documents stay byte-identical to the v1 wire.
    ///
    /// Control-law contract (RUN-1/RUN-2 lesson applies here too): tier
    /// choices are as PROMPT-DEPENDENT as residency and must roll with
    /// the pins — a fossil high-fidelity set is the static-pin fallacy
    /// in bytes/quality form. Residency and tier draw on ONE
    /// rate-distortion budget: higher fidelity costs more bytes per
    /// fault against the same fetch bandwidth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<u32>,
}

impl PlanPin {
    /// Residency-only pin at container-default precision (the v1 shape).
    pub fn residency(layer: u32, expert: u32) -> Self {
        Self {
            layer,
            expert,
            tier: None,
        }
    }

    /// Residency pin with a precision hint into the container's ladder.
    pub fn tiered(layer: u32, expert: u32, tier: u32) -> Self {
        Self {
            layer,
            expert,
            tier: Some(tier),
        }
    }
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
    /// Precision ladder index for every expert NOT in `pin_list` — the
    /// beat-WASTE knob. Misses are unpinned by definition, so this is
    /// where the fetch bytes actually are: `default_tier` pointed at a
    /// small-quant bank shrinks bytes-per-miss for the whole cold tail
    /// while per-pin `tier` keeps the hot set high-fidelity — the
    /// rate-distortion split a uniform-quant streamer structurally
    /// cannot make. `None` = the container's own default bank (the v1
    /// behavior); serde omits it, so documents without it stay
    /// byte-identical to the v1 wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tier: Option<u32>,
}

impl PlanFileDocument {
    pub fn new(budget_bytes: u64, window_k: u32, pin_list: Vec<PlanPin>) -> Self {
        Self {
            version: PLAN_FILE_VERSION,
            budget_bytes,
            window_k,
            pin_list,
            default_tier: None,
        }
    }

    /// Set the ladder index unpinned experts fetch from.
    pub fn with_default_tier(mut self, tier: u32) -> Self {
        self.default_tier = Some(tier);
        self
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
            vec![PlanPin::residency(0, 7), PlanPin::residency(25, 183)],
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

    /// what this catches (precision-hint extension, 2026-08-01): a
    /// tier-less pin must serialize WITHOUT any `tier` key — byte-
    /// identical to the v1 wire her deployed consumer parses — while a
    /// tiered pin carries `"tier":N` literally; and a v1 document with
    /// no tier fields must still parse (tier = None). Either direction
    /// breaking means the two sides need a lockstep upgrade, which this
    /// extension exists to avoid.
    #[test]
    fn tier_is_optional_on_the_wire_and_v1_documents_still_parse() {
        let doc = PlanFileDocument::new(
            1_000,
            8,
            vec![PlanPin::residency(3, 44), PlanPin::tiered(3, 45, 2)],
        );
        let json = serde_json::to_string(&doc).expect("serialize");
        assert!(
            json.contains("{\"layer\":3,\"expert\":44}"),
            "tier-less pin must omit the tier key entirely: {json}"
        );
        assert!(
            json.contains("{\"layer\":3,\"expert\":45,\"tier\":2}"),
            "tiered pin must carry the literal tier key: {json}"
        );
        let back: PlanFileDocument = serde_json::from_str(&json).expect("round trip");
        assert_eq!(back, doc);

        // The exact v1 document shape (pre-extension) parses unchanged.
        let v1 = r#"{"version":1,"budget_bytes":1000,"window_k":8,"pin_list":[{"layer":3,"expert":44}]}"#;
        let parsed: PlanFileDocument = serde_json::from_str(v1).expect("v1 parse");
        assert_eq!(parsed.pin_list, vec![PlanPin::residency(3, 44)]);
        assert_eq!(parsed.pin_list[0].tier, None);
        assert_eq!(parsed.default_tier, None);
    }

    /// what this catches (beat-WASTE knob): `default_tier` — the ladder
    /// index for the entire UNPINNED cold tail — must appear literally
    /// on the wire when set and vanish entirely when not, and a
    /// document carrying it must round-trip. Same no-lockstep contract
    /// as the per-pin tier: absent = byte-identical v1.
    #[test]
    fn default_tier_is_optional_on_the_wire() {
        let plain = PlanFileDocument::new(500, 4, vec![PlanPin::residency(0, 1)]);
        let plain_json = serde_json::to_string(&plain).expect("serialize");
        assert!(
            !plain_json.contains("default_tier"),
            "unset default_tier must be omitted: {plain_json}"
        );

        let tiered =
            PlanFileDocument::new(500, 4, vec![PlanPin::tiered(0, 1, 0)]).with_default_tier(2);
        let json = serde_json::to_string(&tiered).expect("serialize");
        assert!(
            json.contains("\"default_tier\":2"),
            "set default_tier must be a literal wire key: {json}"
        );
        let back: PlanFileDocument = serde_json::from_str(&json).expect("round trip");
        assert_eq!(back, tiered);
        assert_eq!(back.default_tier, Some(2));
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

        let first = PlanFileDocument::new(1_000, 8, vec![PlanPin::residency(1, 2)]);
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
            (0..184).map(|e| PlanPin::residency(0, e)).collect(),
        );
        write_plan_file(&path, &second).expect("rewrite");
        let read2: PlanFileDocument =
            serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
        assert_eq!(read2, second, "rewrite must fully replace the document");
        assert_eq!(read2.pin_list.len(), 184);
    }
}
