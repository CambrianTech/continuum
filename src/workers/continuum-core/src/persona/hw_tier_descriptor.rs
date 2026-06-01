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

/// Tier category — the 3-plan framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/persona/HwTierCategory.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum HwTierCategory {
    /// Intel laptops, low-end hardware. Inference is small + slow OR
    /// routed via grid to a Base/Pro peer. Video still possible via
    /// grid-inference (WebRTC/animation client-side; inference remote).
    Floor,
    /// MacBook M-series unified-memory. The design center. Local-leaning.
    Base,
    /// M-series Pro/Max, future unified-memory PCs. Local + hosts
    /// inference for Floor/Base peers via airc.
    Pro,
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
    export_to = "../../../shared/generated/persona/HwTierDescriptor.ts"
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
            },
            SchemaField {
                name: "label".to_string(),
                field_type: FieldType::String,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
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
            },
            SchemaField {
                name: "localVideoCapable".to_string(),
                field_type: FieldType::Boolean,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
            },
            SchemaField {
                name: "minParamsBMeaningful".to_string(),
                field_type: FieldType::Number,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
            },
            SchemaField {
                name: "maxParamsBFits".to_string(),
                field_type: FieldType::Number,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
            },
            SchemaField {
                name: "unifiedMemoryGib".to_string(),
                field_type: FieldType::Number,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
            },
            SchemaField {
                name: "discreteVramGib".to_string(),
                field_type: FieldType::Number,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
            },
            SchemaField {
                name: "note".to_string(),
                field_type: FieldType::String,
                indexed: false,
                unique: false,
                nullable: true,
                max_length: None,
            },
        ]);
        CollectionSchema {
            collection: Self::COLLECTION.to_string(),
            fields,
            indexes: vec![],
        }
    }
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
        assert!(cat.indexed, "category must be indexed for tier-bucket queries");
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
            category: HwTierCategory::Base,
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
        let json = serde_json::to_string(&HwTierCategory::Floor).expect("ser Floor");
        assert_eq!(json, "\"floor\"");
        let json = serde_json::to_string(&HwTierCategory::Base).expect("ser Base");
        assert_eq!(json, "\"base\"");
        let json = serde_json::to_string(&HwTierCategory::Pro).expect("ser Pro");
        assert_eq!(json, "\"pro\"");
    }
}
