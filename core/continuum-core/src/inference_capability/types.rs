//! Wire types for grid inference routing. ts-rs exports for PR-2's grid wire.
//!
//! All types are `serde_json`-friendly + ts-rs camelCase; the future grid
//! transport (PR-2) carries them across the tailscale mesh; PR-3's router
//! consumes them via the registry.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One inference backend identifier. NOT a const enum — registered as
/// `String` so new backends (tflite, mlx, candle-vulkan, etc.) plug in
/// without a schema change. The convenience consts in `kinds::*` are
/// stable names for the backends that exist today.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/InferenceKind.ts"
)]
pub struct InferenceKind(pub String);

impl InferenceKind {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for InferenceKind {
    fn from(s: &str) -> Self {
        InferenceKind(s.to_string())
    }
}

impl From<String> for InferenceKind {
    fn from(s: String) -> Self {
        InferenceKind(s)
    }
}

/// Stable name aliases for today's backends. Use these when you know the
/// backend at compile time; the registry still accepts arbitrary
/// `InferenceKind(String)` values.
pub mod kinds {
    pub const LLAMACPP: &str = "llamacpp";
    pub const CANDLE: &str = "candle";
    pub const ORT_VISION: &str = "ort-vision";
    pub const ORT_TTS: &str = "ort-tts";
    pub const ORT_STT: &str = "ort-stt";
    pub const ORT_EMBEDDING: &str = "ort-embedding";
}

/// Coarse latency bucket the supervisor uses to score job placement. PR-3's
/// router weights this against RTT cost when picking a node.
///
/// `Local` = under-1ms (in-process). `Fast` = sub-10ms (same machine, ipc).
/// `Mesh` = single-digit-ms (LAN, tailscale local). `Wan` = 50ms+ (tailscale
/// across regions). Not numeric milliseconds because hardware-class buckets
/// are stable across deployments while raw ms vary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/LatencyClass.ts"
)]
pub enum LatencyClass {
    Local,
    Fast,
    Mesh,
    Wan,
}

/// Hardware profile a node's supervisor probes at boot + on hardware-change
/// events. Carried in `probe_inference_capabilities` to derive the
/// capability list. Pure data — the runtime probe writes this; tests
/// synthesize it for the four hardware tiers vhsm-d1f4 named.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/HardwareProfile.ts"
)]
pub struct HardwareProfile {
    /// Human-readable platform identifier ("macos-arm64", "linux-x86_64-cuda",
    /// "macos-arm64-m5pro", "linux-x86_64-blackwell"). Free-form; the
    /// supervisor probe sets this from sysinfo + GPU vendor strings.
    pub platform: String,
    /// Metal device available (any Apple Silicon).
    pub has_metal: bool,
    /// CUDA device available (NVIDIA).
    pub has_cuda: bool,
    /// Vulkan device available (AMD or non-CUDA NVIDIA on Linux/Windows).
    pub has_vulkan: bool,
    /// Free VRAM in bytes. 0 when no discrete/unified GPU memory. Sourced
    /// from the GPU memory manager's live probe (`GpuMemoryManager::stats`).
    #[ts(type = "number")]
    pub free_vram_bytes: u64,
    /// Total VRAM in bytes (for capacity scoring). 0 when not applicable.
    #[ts(type = "number")]
    pub total_vram_bytes: u64,
    /// CPU core count. Set even on GPU-equipped nodes; PR-3 uses it as a
    /// tiebreaker when GPU capacity is similar.
    #[ts(type = "number")]
    pub cpu_cores: u32,
    /// System RAM in bytes (the resource pool the broker meters for
    /// non-GPU work — embeddings, vision pre/postproc, TTS spectrogram).
    #[ts(type = "number")]
    pub system_ram_bytes: u64,
}

/// One inference capability this node can take. Composed by
/// `probe_inference_capabilities` from a `HardwareProfile`; advertised by
/// PR-2's grid announcer; scored by PR-3's router.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/InferenceCapability.ts"
)]
pub struct InferenceCapability {
    /// Backend kind (llamacpp / candle / ort-* / etc.).
    pub kind: InferenceKind,
    /// Free VRAM bytes the supervisor reports as available for this
    /// capability RIGHT NOW. Updated live by the probe; PR-2 announces
    /// at broker-paced intervals; PR-3 uses this for capacity matching.
    #[ts(type = "number")]
    pub free_vram_bytes: u64,
    /// Number of inference leases currently held against this capability.
    /// PR-3 uses (free_vram + current_lease_count) to estimate "can take
    /// one more job" without overcommitting.
    #[ts(type = "number")]
    pub current_lease_count: u32,
    /// Latency class for a local invocation of this capability. Always
    /// `LatencyClass::Local` when produced by the local probe; PR-3's
    /// router pulls RTT-derived classes for remote nodes from the grid
    /// transport's live measurements.
    pub latency_class: LatencyClass,
}

/// All inference capabilities one node advertises. Keyed in the registry
/// by `node_id` so PR-2/PR-3 can dedupe per-node updates.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/NodeCapability.ts"
)]
pub struct NodeCapability {
    /// Tailnet-stable node identifier (the same id the grid transport
    /// uses for routing). For the local node, supervisor-assigned at boot.
    pub node_id: String,
    /// Hardware profile the supervisor probed for this node.
    pub hardware: HardwareProfile,
    /// What this node can take. Ordered for deterministic serialization,
    /// not by priority — PR-3's router does its own scoring.
    pub capabilities: Vec<InferenceCapability>,
    /// Unix-ms timestamp this profile was last refreshed. Stale entries
    /// (older than the registry's TTL) get evicted in PR-2.
    #[ts(type = "number")]
    pub last_updated_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: `InferenceKind` round-trips as a plain string,
    /// not a discriminated-union enum. The grid wire treats backend names
    /// as opaque labels so PR-2 doesn't need a schema bump when a new
    /// backend (tflite, mlx) is added.
    #[test]
    fn inference_kind_serializes_as_string() {
        let k = InferenceKind::from("llamacpp");
        let j = serde_json::to_string(&k).unwrap();
        assert_eq!(j, "\"llamacpp\"", "got: {j}");
        let back: InferenceKind = serde_json::from_str("\"candle\"").unwrap();
        assert_eq!(back.as_str(), "candle");
    }

    /// What this catches: arbitrary backend names parse cleanly. Pinning
    /// the no-hardcoded-enums contract — registries can add backends
    /// without code changes here.
    #[test]
    fn inference_kind_accepts_arbitrary_names() {
        for name in &["tflite", "mlx", "candle-vulkan", "unknown-future-backend"] {
            let k = InferenceKind::from(*name);
            assert_eq!(k.as_str(), *name);
            let j = serde_json::to_string(&k).unwrap();
            let back: InferenceKind = serde_json::from_str(&j).unwrap();
            assert_eq!(back, k);
        }
    }

    /// What this catches: LatencyClass serializes as lowercase, matching
    /// what PR-2's grid wire will emit + what PR-3's router consumes.
    #[test]
    fn latency_class_serializes_as_lowercase() {
        for (variant, expected) in &[
            (LatencyClass::Local, "\"local\""),
            (LatencyClass::Fast, "\"fast\""),
            (LatencyClass::Mesh, "\"mesh\""),
            (LatencyClass::Wan, "\"wan\""),
        ] {
            assert_eq!(
                serde_json::to_string(variant).unwrap(),
                *expected,
                "{variant:?}"
            );
        }
    }

    /// What this catches: LatencyClass orders Local < Fast < Mesh < Wan,
    /// so PR-3's router can compare buckets directly.
    #[test]
    fn latency_class_orders_local_before_wan() {
        assert!(LatencyClass::Local < LatencyClass::Fast);
        assert!(LatencyClass::Fast < LatencyClass::Mesh);
        assert!(LatencyClass::Mesh < LatencyClass::Wan);
    }

    /// What this catches: HardwareProfile round-trips with camelCase wire
    /// names. PR-2's grid serialization depends on field-name stability.
    #[test]
    fn hardware_profile_serde_camelcase() {
        let h = HardwareProfile {
            platform: "macos-arm64-m5pro".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 32_000_000_000,
            total_vram_bytes: 48_000_000_000,
            cpu_cores: 16,
            system_ram_bytes: 64_000_000_000,
        };
        let j = serde_json::to_string(&h).unwrap();
        assert!(j.contains("\"hasMetal\":true"));
        assert!(j.contains("\"freeVramBytes\":32000000000"));
        assert!(j.contains("\"systemRamBytes\":64000000000"));
        let back: HardwareProfile = serde_json::from_str(&j).unwrap();
        assert_eq!(back, h);
    }

    /// What this catches: InferenceCapability full round-trip with the
    /// dynamic kind + latency class. PR-2 announces these over the wire;
    /// PR-3 deserializes from peer announcements.
    #[test]
    fn inference_capability_serde_round_trip() {
        let c = InferenceCapability {
            kind: InferenceKind::from(kinds::LLAMACPP),
            free_vram_bytes: 24_000_000_000,
            current_lease_count: 2,
            latency_class: LatencyClass::Local,
        };
        let j = serde_json::to_string(&c).unwrap();
        assert!(j.contains("\"kind\":\"llamacpp\""));
        assert!(j.contains("\"freeVramBytes\":24000000000"));
        assert!(j.contains("\"currentLeaseCount\":2"));
        assert!(j.contains("\"latencyClass\":\"local\""));
        let back: InferenceCapability = serde_json::from_str(&j).unwrap();
        assert_eq!(back, c);
    }

    /// What this catches: `kinds::*` constants align with the strings
    /// PR-2/PR-3 will compare against. Renaming a const without updating
    /// the wire value would silently break peer registry lookups across
    /// the mesh. Pin every const to its expected wire string.
    #[test]
    fn kinds_consts_match_expected_wire_strings() {
        assert_eq!(kinds::LLAMACPP, "llamacpp");
        assert_eq!(kinds::CANDLE, "candle");
        assert_eq!(kinds::ORT_VISION, "ort-vision");
        assert_eq!(kinds::ORT_TTS, "ort-tts");
        assert_eq!(kinds::ORT_STT, "ort-stt");
        assert_eq!(kinds::ORT_EMBEDDING, "ort-embedding");
    }

    /// What this catches: InferenceKind is hashable + usable as a HashMap
    /// key. PR-3's router will likely group capabilities by kind across
    /// nodes; if InferenceKind ever loses Hash/Eq, those data structures
    /// stop compiling. Lock the bound here.
    #[test]
    fn inference_kind_is_hashable() {
        use std::collections::HashMap;
        let mut m: HashMap<InferenceKind, u32> = HashMap::new();
        m.insert(InferenceKind::from(kinds::LLAMACPP), 1);
        m.insert(InferenceKind::from(kinds::CANDLE), 2);
        assert_eq!(m.get(&InferenceKind::from("llamacpp")), Some(&1));
        assert_eq!(m.get(&InferenceKind::from("candle")), Some(&2));
        assert_eq!(m.get(&InferenceKind::from("nope")), None);
    }

    /// What this catches: NodeCapability carries node_id + hardware +
    /// capabilities + last_updated_ms. The registry keys off `node_id`;
    /// PR-2's announcer updates `last_updated_ms`; PR-3's router uses
    /// stale-detection against it.
    #[test]
    fn node_capability_carries_full_advertisement() {
        let n = NodeCapability {
            node_id: "tailnet-node-abc123".into(),
            hardware: HardwareProfile {
                platform: "linux-x86_64-blackwell".into(),
                has_metal: false,
                has_cuda: true,
                has_vulkan: false,
                free_vram_bytes: 80_000_000_000,
                total_vram_bytes: 96_000_000_000,
                cpu_cores: 32,
                system_ram_bytes: 256_000_000_000,
            },
            capabilities: vec![
                InferenceCapability {
                    kind: InferenceKind::from(kinds::LLAMACPP),
                    free_vram_bytes: 80_000_000_000,
                    current_lease_count: 0,
                    latency_class: LatencyClass::Local,
                },
                InferenceCapability {
                    kind: InferenceKind::from(kinds::CANDLE),
                    free_vram_bytes: 80_000_000_000,
                    current_lease_count: 0,
                    latency_class: LatencyClass::Local,
                },
            ],
            last_updated_ms: 1_715_625_600_000,
        };
        let j = serde_json::to_string(&n).unwrap();
        assert!(j.contains("\"nodeId\":\"tailnet-node-abc123\""));
        assert!(j.contains("\"lastUpdatedMs\":1715625600000"));
        assert!(j.contains("\"capabilities\":[{"));
        let back: NodeCapability = serde_json::from_str(&j).unwrap();
        assert_eq!(back, n);
    }
}
