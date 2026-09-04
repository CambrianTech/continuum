//! HwTierDescriptor — the editable, shareable, ORM-stored description
//! of one hardware tier.
//!
//! Distinct from [`crate::inference_capability::HwCapabilityTier`] (the
//! discriminant enum) — the enum answers "which tier am I?", this
//! struct answers "what does that tier MEAN?". The catalog of
//! descriptors lives in the `hw_tiers` ORM collection; one row per
//! tier; rows authored as `seeds/hw_tiers/*.json` (slice 2), ingested
//! into the ORM on first boot.
//!
//! Three categories per Joel's 2026-06-01 three-plan framing:
//! - **Floor** — Intel + low-end laptops. Video via grid-inference.
//! - **Base** — MacBook M-series. Local-leaning. Current design center.
//! - **Pro** — M-series Pro/Max + future unified-memory PCs (Spark,
//!   Strix Halo, etc.). Local + grid-host for floor/base peers.
//!
//! References: [[orm-everything-not-hand-edited-files]],
//! [[authored-data-vs-procedural-projection]].

use crate::orm::types::{CollectionSchema, FieldType, SchemaField};
use crate::orm::{base_entity_fields, OrmEntity};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Tier category — Joel's 5-variant hierarchy (2026-06-01, #133).
///
/// Replaces the earlier 3-plan framing (Floor/Base/Pro) with a richer
/// taxonomy that maps directly to hardware classes the substrate
/// actually targets. The substrate ships LCD as the always-works safe
/// mode; everything else lights up on capable hardware. Per [[lcd-model-
/// qwen25-05b-and-foundry-lora]] and [[optimizing-for-low-end-compounds-
/// on-high-end]], obsessive optimization on the Compat tier transfers
/// upward to every higher tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/HwTierCategory.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum HwTierCategory {
    /// **LCD / safe / compatibility mode.** Works everywhere — Intel
    /// Mac, CPU-only, anything weak. The substrate's lowest-common-
    /// denominator. Multi-persona still expected via small models +
    /// LoRA paging + grid-inference offload for what local can't carry.
    /// Joel (2026-06-01): "This LCD is the lowest default. This is
    /// maybe the compatibility mode enum value."
    Compat,
    /// Apple Silicon M1-M4 baseline. Unified memory, capable Metal
    /// backend. Local-leaning. The design center for typical user
    /// hardware in 2026.
    MSeries,
    /// M-series Pro/Max/Ultra. Headroom for 7B-14B local models,
    /// multi-persona at full quality, hosts inference for Compat
    /// peers via the grid.
    MSeriesPro,
    /// NVIDIA discrete GPUs. Spans Sm60 (Pascal / 1080Ti) through
    /// Sm120 (Blackwell / 5090). Wide capability range; per-device
    /// VRAM in the descriptor narrows it.
    Cuda,
    /// Cloud-hosted inference (Anthropic, OpenAI, etc.). Not local
    /// compute — rendering stays local; only the model lives in the
    /// cloud. Always eligible per
    /// [[inference-is-an-adapter-always-in-the-loop]].
    Cloud,
}

/// One hardware tier's descriptor — flat row in the `hw_tiers`
/// collection. Storage shape mirrors the JSON authoring shape.
///
/// `Eq` is intentionally NOT derived — `f32` fields can hold NaN. Use
/// `PartialEq` for tests; bit-exact equality is meaningless for the
/// fraction-of-a-billion params_b sliders anyway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/HwTierDescriptor.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct HwTierDescriptor {
    /// Stable domain-natural key matching `HwCapabilityTier` variants
    /// in snake_case form, e.g. `"cpu_only"`, `"m1_uma_8gb"`,
    /// `"m3_uma_pro_max"`, `"mac_intel_metal_discrete"`, `"sm60"`,
    /// `"sm120"`, `"vulkan_amd"`, `"cloud"`. NOT the same as the
    /// record's `id` field (which is the UUID PK from BaseEntity).
    pub tier_id: String,
    /// Human label shown in UIs and AI-introspection output.
    pub label: String,
    /// Three-plan framing.
    pub category: HwTierCategory,
    /// Whether the host can render live persona video LOCALLY at this
    /// tier. Floor=false (renders via grid-inference); Base/Pro=true.
    /// WebRTC + animation are already optimized; this flag is about
    /// having enough local inference throughput to drive a real-time
    /// avatar pipeline without offloading.
    pub local_video_capable: bool,
    /// Smallest model in billions of params worth running here. CpuOnly
    /// might be 0.5; M3UmaProMax might be 4.0.
    pub min_params_b_meaningful: f32,
    /// Largest model in billions of params that practically fits.
    /// Useful for capability_floor matching in [[role_templates]].
    pub max_params_b_fits: f32,
    /// Optional: unified-memory size in GiB if applicable.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unified_memory_gib: Option<u32>,
    /// Optional: discrete VRAM in GiB if applicable.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discrete_vram_gib: Option<u32>,
    /// Free-form note from the catalog. Future builds may surface this
    /// in the user-facing tier picker.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl OrmEntity for HwTierDescriptor {
    const COLLECTION: &'static str = "hw_tiers";

    fn collection_schema() -> CollectionSchema {
        // BaseEntity fields (id/createdAt/updatedAt/version) come from
        // the shared helper so the storage shape stays in lockstep with
        // every other entity in the system, Rust-authored or TS-
        // authored. Per Joel's "rust entities adhering to some base
        // that ts also supports" (2026-06-01).
        let mut fields = base_entity_fields();
        // Entity-specific fields. `tier_id` is the domain-natural key
        // — unique + indexed because spawner/probe code queries
        // `WHERE tier_id = 'm1_uma_8gb'` constantly. Distinct from the
        // record's UUID `id` (BaseEntity primary).
        fields.extend(vec![
            SchemaField {
                name: "tierId".to_string(),
                field_type: FieldType::String,
                indexed: true,
                unique: true,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            SchemaField {
                name: "label".to_string(),
                field_type: FieldType::String,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            // category is indexed for tier-bucket queries
            // ("give me all Pro tiers").
            SchemaField {
                name: "category".to_string(),
                field_type: FieldType::String,
                indexed: true,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            SchemaField {
                name: "localVideoCapable".to_string(),
                field_type: FieldType::Boolean,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            SchemaField {
                name: "minParamsBMeaningful".to_string(),
                field_type: FieldType::Number,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            SchemaField {
                name: "maxParamsBFits".to_string(),
                field_type: FieldType::Number,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            },
            SchemaField {
                name: "unifiedMemoryGib".to_string(),
                field_type: FieldType::Number,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            },
            SchemaField {
                name: "discreteVramGib".to_string(),
                field_type: FieldType::Number,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            },
            SchemaField {
                name: "note".to_string(),
                field_type: FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
                foreign_key: None,
            },
        ]);
        CollectionSchema {
            collection: Self::COLLECTION.to_string(),
            fields,
            indexes: vec![],
        }
    }
}

// ── Seed JSON (embedded at compile time) ─────────────────────────
//
// Per [[orm-everything-not-hand-edited-files]]: repo source is JSON
// (human-readable, git-diffable, PR-reviewable), runtime backend is
// the ORM. `include_str!` bakes the seed files into the binary so the
// substrate always ships data + code together — no runtime path-
// discovery, no missing-file failure modes, headless-clean.
//
// Adding a new tier:
//   1. Author `seeds/hw_tiers/<tier_id>.json` (camelCase fields)
//   2. Add a `SEED_*` const here pointing at it via include_str!
//   3. Add the entry to `SEED_FILES` below
//   4. Tests fail loud if the JSON doesn't parse into HwTierDescriptor
//
// On substrate boot, a future spawn-module step ingests these into
// the `hw_tiers` ORM collection if it's empty (slice 3). Right now
// they're available for any caller that wants the defaults.

const SEED_CPU_ONLY: &str = include_str!("../../seeds/hw_tiers/cpu_only.json");
const SEED_MAC_INTEL_METAL_DISCRETE: &str =
    include_str!("../../seeds/hw_tiers/mac_intel_metal_discrete.json");
const SEED_M1_UMA_8GB: &str = include_str!("../../seeds/hw_tiers/m1_uma_8gb.json");
const SEED_M1_UMA_16GB: &str = include_str!("../../seeds/hw_tiers/m1_uma_16gb.json");
const SEED_M3_UMA_PRO_MAX: &str = include_str!("../../seeds/hw_tiers/m3_uma_pro_max.json");
const SEED_M5_UMA_PRO_MAX: &str = include_str!("../../seeds/hw_tiers/m5_uma_pro_max.json");
const SEED_SM60: &str = include_str!("../../seeds/hw_tiers/sm60.json");
const SEED_SM120: &str = include_str!("../../seeds/hw_tiers/sm120.json");
const SEED_CLOUD: &str = include_str!("../../seeds/hw_tiers/cloud.json");

/// Every seed file shipping with this build. Each entry is
/// `(tier_id, raw_json)` for diagnostic clarity when a parse fails —
/// the error message can name the file by its expected tier_id.
pub const SEED_FILES: &[(&str, &str)] = &[
    ("cpu_only", SEED_CPU_ONLY),
    ("mac_intel_metal_discrete", SEED_MAC_INTEL_METAL_DISCRETE),
    ("m1_uma_8gb", SEED_M1_UMA_8GB),
    ("m1_uma_16gb", SEED_M1_UMA_16GB),
    ("m3_uma_pro_max", SEED_M3_UMA_PRO_MAX),
    ("m5_uma_pro_max", SEED_M5_UMA_PRO_MAX),
    ("sm60", SEED_SM60),
    ("sm120", SEED_SM120),
    ("cloud", SEED_CLOUD),
];

/// Parse every embedded seed file into a Vec<HwTierDescriptor>. Returns
/// the first parse error with the file's expected tier_id for diagnosis.
/// Used at boot to populate the `hw_tiers` ORM collection on first run,
/// and at test time as the #125 CI guard (any drift between the Rust
/// struct shape and the seed JSON fails the build).
pub fn parse_seed_descriptors() -> Result<Vec<HwTierDescriptor>, String> {
    SEED_FILES
        .iter()
        .map(|(expected_id, raw)| {
            let descriptor: HwTierDescriptor = serde_json::from_str(raw).map_err(|e| {
                format!(
                    "hw_tiers seed '{}' failed to parse against HwTierDescriptor: {}",
                    expected_id, e
                )
            })?;
            if descriptor.tier_id != *expected_id {
                return Err(format!(
                    "hw_tiers seed '{}.json' has tier_id='{}' — file name and tier_id must match",
                    expected_id, descriptor.tier_id
                ));
            }
            Ok(descriptor)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::OrmEntityRegistry;

    /// Smoke: schema has BaseEntity (4) + entity-specific (9) = 13.
    /// If this count changes, double-check that field WAS intended to
    /// be added/removed — accidental schema drift breaks deployed
    /// databases.
    #[test]
    fn schema_collection_name_and_field_count() {
        let schema = HwTierDescriptor::collection_schema();
        assert_eq!(schema.collection, "hw_tiers");
        assert_eq!(schema.fields.len(), 13);
    }

    /// BaseEntity contract: id/createdAt/updatedAt/version present
    /// with the canonical shapes. Load-bearing — adapters depend on
    /// these for primary key, optimistic concurrency, and recency
    /// queries.
    #[test]
    fn base_entity_fields_are_present() {
        let schema = HwTierDescriptor::collection_schema();
        let names: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
        for base in ["id", "createdAt", "updatedAt", "version"] {
            assert!(
                names.contains(&base),
                "missing BaseEntity field '{}' — got {:?}",
                base,
                names
            );
        }
        let id_field = schema.fields.iter().find(|f| f.name == "id").expect("id");
        assert!(id_field.unique, "id (BaseEntity primary) must be unique");
        assert!(id_field.indexed, "id must be indexed");
    }

    /// Domain key tierId is the natural identifier — unique + indexed,
    /// distinct from the UUID `id` (BaseEntity primary).
    #[test]
    fn tier_id_is_unique_indexed_and_distinct_from_pk() {
        let schema = HwTierDescriptor::collection_schema();
        let tier_id = schema
            .fields
            .iter()
            .find(|f| f.name == "tierId")
            .expect("tierId field");
        assert!(tier_id.unique, "tierId must be unique");
        assert!(tier_id.indexed, "tierId must be indexed");
        assert!(!tier_id.nullable, "tierId must not be nullable");
        // Sanity: id and tierId are separate fields.
        let id_field = schema.fields.iter().find(|f| f.name == "id").expect("id");
        assert_ne!(id_field.name, tier_id.name);
    }

    /// category is indexed for "give me all Pro tiers" queries.
    #[test]
    fn category_field_is_indexed() {
        let schema = HwTierDescriptor::collection_schema();
        let cat = schema
            .fields
            .iter()
            .find(|f| f.name == "category")
            .expect("category field");
        assert!(
            cat.indexed,
            "category must be indexed for tier-bucket queries"
        );
    }

    /// Registers cleanly + resolves via a fresh registry (no global
    /// race under parallel cargo test).
    #[test]
    fn registers_into_orm_registry() {
        let registry = OrmEntityRegistry::new();
        registry
            .register::<HwTierDescriptor>()
            .expect("register HwTierDescriptor");
        let resolved = registry
            .resolve("hw_tiers")
            .expect("hw_tiers resolves via Rust path");
        assert_eq!(resolved.collection, "hw_tiers");
        assert_eq!(resolved.fields.len(), 13);
    }

    /// Round-trips through serde without panic. Field naming
    /// convention (camelCase) propagates to JSON.
    #[test]
    fn serde_roundtrip_uses_camel_case() {
        let descriptor = HwTierDescriptor {
            tier_id: "m1_uma_8gb".to_string(),
            label: "M1 8GB Unified Memory".to_string(),
            category: HwTierCategory::MSeries,
            local_video_capable: true,
            min_params_b_meaningful: 0.5,
            max_params_b_fits: 3.0,
            unified_memory_gib: Some(8),
            discrete_vram_gib: None,
            note: None,
        };
        let json = serde_json::to_string(&descriptor).expect("serialize");
        assert!(json.contains("\"tierId\":\"m1_uma_8gb\""));
        assert!(json.contains("\"localVideoCapable\":true"));
        assert!(json.contains("\"unifiedMemoryGib\":8"));
        // Optional None fields skipped.
        assert!(!json.contains("discreteVramGib"));
        assert!(!json.contains("\"note\""));
        let back: HwTierDescriptor = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, descriptor);
    }

    /// Categories serialize as lowercase strings — matches `#[serde(rename_all = "lowercase")]`.
    #[test]
    fn category_serializes_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&HwTierCategory::Compat).expect("ser Compat"),
            "\"compat\""
        );
        assert_eq!(
            serde_json::to_string(&HwTierCategory::MSeries).expect("ser MSeries"),
            "\"mseries\""
        );
        assert_eq!(
            serde_json::to_string(&HwTierCategory::MSeriesPro).expect("ser MSeriesPro"),
            "\"mseriespro\""
        );
        assert_eq!(
            serde_json::to_string(&HwTierCategory::Cuda).expect("ser Cuda"),
            "\"cuda\""
        );
        assert_eq!(
            serde_json::to_string(&HwTierCategory::Cloud).expect("ser Cloud"),
            "\"cloud\""
        );
    }

    /// CI guard from #125: every embedded seed JSON must parse cleanly
    /// against the HwTierDescriptor Rust struct. If the struct grows a
    /// required field or renames an existing one, this test fails loud
    /// — you cannot ship a binary whose seed data doesn't match its
    /// schema.
    #[test]
    fn all_seed_files_parse_into_descriptors() {
        let descriptors = parse_seed_descriptors().expect("all seeds parse");
        assert!(!descriptors.is_empty(), "no seeds shipped");
        // Sanity: every tier_id is unique within the seed set.
        let mut ids: Vec<_> = descriptors.iter().map(|d| d.tier_id.as_str()).collect();
        ids.sort();
        let unique_count = {
            let mut v = ids.clone();
            v.dedup();
            v.len()
        };
        assert_eq!(
            ids.len(),
            unique_count,
            "duplicate tier_id in seeds — got {:?}",
            ids
        );
    }

    /// 5-variant hierarchy (Joel, 2026-06-01, #133) must have
    /// representatives in each currently-shipping category. Cloud +
    /// Compat are non-negotiable (universal fallback / universal floor).
    /// MSeries + MSeriesPro + Cuda asserted as soon as their seeds ship;
    /// for now the floor is Compat + MSeries + at least one Cuda variant.
    #[test]
    fn seeds_cover_required_categories() {
        let descriptors = parse_seed_descriptors().expect("parse");
        let has = |cat: HwTierCategory| descriptors.iter().any(|d| d.category == cat);
        assert!(has(HwTierCategory::Compat), "no Compat-tier seed shipped");
        assert!(has(HwTierCategory::MSeries), "no MSeries-tier seed shipped");
        assert!(
            has(HwTierCategory::MSeriesPro),
            "no MSeriesPro-tier seed shipped"
        );
        assert!(has(HwTierCategory::Cuda), "no Cuda-tier seed shipped");
        assert!(has(HwTierCategory::Cloud), "no Cloud-tier seed shipped");
    }

    /// Specific anchor seeds must be present — they're load-bearing
    /// for downstream code (spawner, capability gating, etc.).
    /// Removing them silently would break inference routing.
    #[test]
    fn anchor_tiers_are_present() {
        let descriptors = parse_seed_descriptors().expect("parse");
        let ids: std::collections::HashSet<&str> =
            descriptors.iter().map(|d| d.tier_id.as_str()).collect();
        for required in ["cpu_only", "m1_uma_8gb", "m3_uma_pro_max", "sm120", "cloud"] {
            assert!(
                ids.contains(required),
                "anchor tier '{}' missing from seeds (have {:?})",
                required,
                ids
            );
        }
    }

    /// Cross-check: file-name-derived tier_id matches the JSON's
    /// tier_id field. Catches typos / copy-paste errors at build time.
    #[test]
    fn seed_file_names_match_tier_ids() {
        // parse_seed_descriptors() already enforces this, but make it
        // an explicit named assertion for clarity in CI failure logs.
        for (expected_id, raw) in SEED_FILES.iter() {
            let descriptor: HwTierDescriptor = serde_json::from_str(raw)
                .unwrap_or_else(|e| panic!("seed '{}' failed to parse: {}", expected_id, e));
            assert_eq!(
                descriptor.tier_id, *expected_id,
                "seed file '{}.json' has mismatched tier_id '{}'",
                expected_id, descriptor.tier_id
            );
        }
    }
}
