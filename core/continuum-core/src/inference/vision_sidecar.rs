//! Vision SIDECAR lane (#106): a small VL model serving BESIDE the persona lane
//! so every persona has eyes even when the mind's own model is text-only.
//!
//! The main lane serves ONE model (the residents' mind — e.g. Devstral). A
//! text-only mind cannot receive pixels, and the sensory bridge
//! (`VisionDescriptionService` → `cognition/vision-describe`) needs a
//! vision-capable endpoint to turn an observe/look image into words. Before
//! this module, a headless box with a text mind had NO local vision endpoint:
//! `vision_ready: false`, every describe skipped all candidates, and the
//! persona's eyes (the connected eye-node, `perception/observe`) delivered
//! pixels nobody could interpret.
//!
//! The sidecar reuses [`EphemeralServingLane`] (the eval-lane primitive:
//! Drop-kills its child, own scanned port, never touches the live lane) but
//! holds it LONG-LIVED under the serving daemon's ownership. Placement is
//! [`LanePlacement::Cpu`] by design, not fallback: on a single GPU two resident
//! models OOM the Metal command buffer at decode (#59/#175 lesson), and a
//! describe is an occasional, seconds-tolerant call — the misfit-toy CPU RAM is
//! the right home. Promotion to a governed GPU share is the #173/#126 arc.
//!
//! Decision logic is PURE (unit-tested); the daemon calls [`ensure_sidecar`]
//! from its reconcile with the decision's plan. Every "no" is a NAMED reason
//! probed loud — a persona without eyes is a diagnosable state, never a silent
//! one ([[fallbacks-are-illegal-fail-loud]]).

use std::path::PathBuf;

use crate::inference::llama_server::{
    vision_lane_ready, EphemeralServingLane, LanePlacement, ServingTarget,
};
use crate::model_registry::types::Model;
use crate::model_registry::Capability;

/// Port scan base for the vision sidecar — decimal-distant from the live lane
/// (58057) and the eval lane's scan range so a port-collision never routes a
/// describe at the wrong model.
pub const VISION_SIDECAR_BASE_PORT: u16 = 58091;

/// The sidecar's served window. Describe prompts are ONE image + a short
/// instruction; 8k covers the multimodal tokenization of a full-detail frame
/// with generous margin while keeping the KV cost trivial.
// context-budget-exempt: the DESCRIBE sidecar's own served window, sized to its one fixed job (a single image + a short instruction) and budgeted against by SIDECAR_OVERHEAD_BYTES. Not a persona lane and not a bound on any model's cognition
pub const SIDECAR_CTX: u32 = 8192;

/// Host-RAM margin the sidecar demands beyond its weights: mmproj + KV at
/// [`SIDECAR_CTX`] + llama.cpp compute buffers, rounded up. A gate on REAL free
/// bytes, per the eval-lane lesson ([[eval-lane-second-24b-cannot-coexist]]):
/// planning from a stale or optimistic figure SIGKILLs a lane mid-load.
pub const SIDECAR_OVERHEAD_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// A vision row that can actually stand up as the sidecar: artifacts ON DISK
/// (no network fetch inside a reconcile — `models/pull` is the operator/persona
/// verb for that), projector included.
#[derive(Debug, Clone)]
pub struct SidecarCandidate {
    pub model: Model,
    pub gguf: PathBuf,
    pub mmproj: PathBuf,
    /// Weights size on disk — the dominant term of the budget gate.
    pub weights_bytes: u64,
}

/// Why no sidecar can (or need) come up. Every variant is a teaching string in
/// the probe — the difference between "personas are blind" and "personas are
/// blind BECAUSE X; do Y".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarVerdict {
    /// The MAIN lane already sees (a VL mind) — no sidecar needed.
    MainLaneSees,
    /// Stand this candidate up.
    Spawn,
    /// No local vision row has its artifacts on disk. Names what was skipped.
    NoCandidate { skipped: Vec<String> },
    /// A candidate exists but free host memory can't hold it alongside the
    /// live lane. Real numbers, so the operator sees the actual gap.
    NoBudget { need_bytes: u64, free_bytes: u64 },
}

/// Pure decision: should the daemon stand up a vision sidecar this reconcile?
///
/// `main_vision_ready` is the reconcile's verdict on the LIVE lane;
/// `candidate` is the best on-disk vision row (or the skip reasons);
/// `free_host_bytes` is the LIVE available-memory read (never a cached plan
/// figure — the eval-lane SIGKILL lesson).
pub fn plan_sidecar(
    main_vision_ready: bool,
    candidate: Result<&SidecarCandidate, &[String]>,
    free_host_bytes: u64,
) -> SidecarVerdict {
    if main_vision_ready {
        return SidecarVerdict::MainLaneSees;
    }
    match candidate {
        Err(skipped) => SidecarVerdict::NoCandidate {
            skipped: skipped.to_vec(),
        },
        Ok(c) => {
            let need = c.weights_bytes.saturating_add(SIDECAR_OVERHEAD_BYTES);
            if free_host_bytes < need {
                SidecarVerdict::NoBudget {
                    need_bytes: need,
                    free_bytes: free_host_bytes,
                }
            } else {
                SidecarVerdict::Spawn
            }
        }
    }
}

/// Find the best on-disk vision candidate among `rows`, excluding the model the
/// MAIN lane serves (that case is `MainLaneSees` or "pin it" territory, never a
/// duplicate second copy of the same weights). Returns the first row whose GGUF
/// AND mmproj both resolve locally, or the named skip reasons for the probe.
pub fn find_candidate(
    rows: &[Model],
    active_model: Option<&str>,
) -> Result<SidecarCandidate, Vec<String>> {
    let mut skipped = Vec::new();
    for m in rows {
        if !m.has(Capability::Vision) {
            continue;
        }
        if active_model == Some(m.id.as_str()) {
            skipped.push(format!("{}: is the main lane's model", m.id));
            continue;
        }
        let Some(gguf) = crate::model_registry::artifacts::resolve_gguf_for_model(m) else {
            skipped.push(format!("{}: no local GGUF", m.id));
            continue;
        };
        let Some(mmproj) = crate::model_registry::artifacts::resolve_mmproj_for_model(m) else {
            skipped.push(format!("{}: no mmproj projector on disk", m.id));
            continue;
        };
        let weights_bytes = std::fs::metadata(&gguf).map(|md| md.len()).unwrap_or(0);
        if weights_bytes == 0 {
            skipped.push(format!("{}: GGUF unreadable/empty at {}", m.id, gguf.display()));
            continue;
        }
        return Ok(SidecarCandidate {
            model: m.clone(),
            gguf,
            mmproj,
            weights_bytes,
        });
    }
    Err(skipped)
}

/// The verified, live sidecar the snapshot publishes: the routing address and
/// the model it serves. Existence == the multimodal endpoint ANSWERED
/// (`/props modalities.vision`), never just "a process spawned".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarLane {
    pub base_url: String,
    pub model_id: String,
}

/// Bring the sidecar up (or verify the existing one), long-lived in `slot`.
///
/// Idempotent per reconcile tick: an existing lane serving the planned model
/// whose multimodal endpoint still answers is kept as-is (no churn); a dead,
/// wrong-model, or sight-lost lane is dropped (child killed via `Drop`) and
/// respawned. Any failure returns the NAMED reason and leaves `slot` empty —
/// the snapshot then publishes `vision_ready: false` honestly.
pub async fn ensure_sidecar(
    slot: &mut Option<EphemeralServingLane>,
    cand: &SidecarCandidate,
) -> Result<SidecarLane, String> {
    // Keep a healthy incumbent: same model + endpoint still claims sight.
    if let Some(existing) = slot.as_ref() {
        match existing.multimodal_support().await {
            Ok(props) => {
                let verified = vision_lane_ready(true, true, props).unwrap_or(false);
                if verified && existing.active_model().await.ok().flatten().as_deref()
                    == Some(cand.model.id.as_str())
                {
                    return Ok(SidecarLane {
                        base_url: existing.v1_url(),
                        model_id: cand.model.id.clone(),
                    });
                }
            }
            Err(_) => { /* dead lane — fall through to respawn */ }
        }
        // Wrong model / lost sight / unreachable: drop kills the child.
        *slot = None;
    }

    let target = ServingTarget {
        model: cand.model.clone(),
        context_window: SIDECAR_CTX,
        lanes: 1,
        adapters: Vec::new(),
        placement: LanePlacement::Cpu,
        expert_placement: None,
        resident_override: None, // vision sidecar serves as-shipped; no device-fit override
    };
    let lane = EphemeralServingLane::spawn(&target, VISION_SIDECAR_BASE_PORT)
        .await
        .map_err(|e| format!("sidecar spawn failed: {e}"))?;

    // ENDPOINT truth (#106): only the process itself can confirm the projector
    // loaded. Unverifiable ≠ working.
    let props = lane
        .multimodal_support()
        .await
        .map_err(|e| format!("sidecar up but /props unreadable: {e}"))?;
    match vision_lane_ready(true, true, props) {
        Ok(true) => {
            let out = SidecarLane {
                base_url: lane.v1_url(),
                model_id: cand.model.id.clone(),
            };
            *slot = Some(lane);
            Ok(out)
        }
        Ok(false) => Err("sidecar row lost its Vision declaration mid-spawn".to_string()),
        Err(why) => Err(format!("sidecar cannot verifiably see: {why}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::types::Model;

    fn vision_row(id: &str) -> Model {
        use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
        let mut capabilities = std::collections::BTreeSet::new();
        capabilities.insert(Capability::Vision);
        Model {
            id: id.to_string(),
            name: None,
            provider: crate::inference::llama_server::PROVIDER_ID.to_string(),
            arch: Arch::Qwen2,
            context_window: 32_768,
            max_output_tokens: 4096,
            tokens_per_second: 0.0,
            capabilities,
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            hf_source: None,
            gguf_local_path: None,
            chat_template: None,
            stop_sequences: Vec::new(),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            mmproj_local_path: None,
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
        }
    }

    // what this catches: the sidecar must never duplicate the main lane's model
    // (a second copy of the same weights) and must skip rows with no on-disk
    // artifacts WITH a named reason — the difference between diagnosable
    // blindness and silent blindness.
    #[test]
    fn candidate_skips_active_model_and_names_artifact_gaps() {
        let rows = vec![vision_row("vl-model-a"), vision_row("vl-model-b")];
        let out = find_candidate(&rows, Some("vl-model-a"));
        // Neither resolves artifacts in a test env; the ACTIVE one is skipped
        // for being active, the other for missing artifacts.
        let skipped = out.expect_err("no artifacts on disk in tests");
        assert!(skipped.iter().any(|s| s.contains("is the main lane's model")));
        assert!(skipped.iter().any(|s| s.contains("no local GGUF")));
    }

    // what this catches: the budget gate compares REAL free bytes against
    // weights + overhead — an over-budget spawn is refused with both numbers
    // (the eval-lane second-24B SIGKILL class), and a seeing main lane never
    // spawns a redundant sidecar.
    #[test]
    fn plan_gates_on_budget_and_main_lane_sight() {
        let cand = SidecarCandidate {
            model: vision_row("vl"),
            gguf: PathBuf::from("/x.gguf"),
            mmproj: PathBuf::from("/x-mmproj.gguf"),
            weights_bytes: 5 * 1024 * 1024 * 1024,
        };
        // Main lane sees → no sidecar regardless of budget.
        assert_eq!(
            plan_sidecar(true, Ok(&cand), u64::MAX),
            SidecarVerdict::MainLaneSees
        );
        // Not enough free RAM → refused with the real gap.
        let need = cand.weights_bytes + SIDECAR_OVERHEAD_BYTES;
        assert_eq!(
            plan_sidecar(false, Ok(&cand), need - 1),
            SidecarVerdict::NoBudget {
                need_bytes: need,
                free_bytes: need - 1
            }
        );
        // Enough → spawn.
        assert_eq!(plan_sidecar(false, Ok(&cand), need), SidecarVerdict::Spawn);
        // No candidate → the named reasons ride through.
        let reasons = vec!["vl: no local GGUF".to_string()];
        assert_eq!(
            plan_sidecar(false, Err(&reasons), u64::MAX),
            SidecarVerdict::NoCandidate { skipped: reasons }
        );
    }
}
