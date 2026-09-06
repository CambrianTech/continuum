//! The LIVE model universe — the runtime-mutable layer over the immutable seed.
//!
//! [`super::singleton`] holds the curated catalog in a `OnceLock`: loaded once
//! at boot, immutable, read as `&Model` by ~24 call sites that only ever need
//! the *static* facts (arch, context window, chat template). That immutability
//! is correct for those readers — and it is exactly why, on its own, adding a
//! model would mean a reboot.
//!
//! This module is the additive answer. [`ModelCatalog`] owns a
//! `watch::Sender<Arc<CatalogSnapshot>>` — the canonical concurrent shape from
//! `docs/architecture/CONCURRENCY-STYLE-GUIDE.md`. It is SEEDED from the
//! immutable registry at boot, then MUTATED at runtime by the rich `models/*`
//! command surface (discover / pull / try / register). Readers hold a
//! `watch::Receiver` and `borrow()` an `Arc<CatalogSnapshot>` — zero-copy,
//! lock-free, reads never block writes. A mutation builds the next snapshot,
//! bumps `generation`, and publishes a fresh `Arc` (copy-on-write). New models
//! land in the snapshot live; subscribers react over the channel. No reboot.
//!
//! ## Provenance (where each field comes from)
//!
//! - **seed** — the curated residue catalog (`catalog.rs`), validated into the
//!   immutable registry. The `Model` facts.
//! - **hydrated** — the artifact's own metadata: GGUF headers
//!   ([`super::hydrate`]) + provider `/v1/models` ([`super::discovery`]), folded
//!   onto the `Model` before it reaches a snapshot.
//! - **live status** — [`ModelStatus`]: the facts that only exist at runtime
//!   (is the artifact on disk? has `models/try` verified it? what tok/s did we
//!   actually measure?). The immutable catalog *cannot* carry these; they change
//!   while the process runs.
//!
//! ## Not a parallel allocator
//!
//! There is no other owner of live model state — the registry is the immutable
//! seed, and `ModelsModule` now HOLDS this one `ModelCatalog` (the rich
//! `models/*` commands capture the same `Arc`). [`ModelCatalog`] is the single
//! owner of a concern that previously had no live home, not a second manager
//! competing with one.

use std::collections::BTreeMap;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use super::registry::Registry;
use super::types::{Model, ProviderKind};

/// Whether a model is usable *right now* on this host. The one runtime fact a
/// `models/pull` flips and a `models/list` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/Availability.ts"
)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    /// Artifact present on disk (local) or remote endpoint configured (cloud) —
    /// the model can be served without first fetching anything.
    Ready,
    /// A local model whose GGUF is not on disk yet. `models/pull` acquires it,
    /// then flips this to [`Availability::Ready`].
    NotDownloaded,
}

/// What `models/try` learned by actually loading a model and running a smoke
/// inference against it. Absent until verification runs; attached to the live
/// status once it does. This is the "can we actually handle it?" record.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/VerifyReport.ts"
)]
pub struct VerifyReport {
    /// A minimal text generation completed.
    pub text_ok: bool,
    /// A text+image probe completed. `None` when the model does not advertise
    /// [`super::Capability::Vision`] (nothing to verify), `Some(false)` when it
    /// claims vision but the probe failed.
    pub vision_ok: Option<bool>,
    /// Tokens/sec measured during the smoke run — the live correction to the
    /// catalog's startup estimate.
    pub measured_tps: Option<f32>,
    /// Human-readable summary of the run (what worked, what failed, why).
    pub detail: String,
}

/// The runtime-mutable facts about one model. Distinct from [`Model`] (the
/// static seed facts) precisely because these change while the process runs.
#[derive(Debug, Clone)]
pub struct ModelStatus {
    pub availability: Availability,
    /// Last verification result, if `models/try` has run against this model.
    pub verified: Option<VerifyReport>,
}

/// One model in the live universe: its static facts plus its runtime status.
#[derive(Debug, Clone)]
pub struct LiveModel {
    pub model: Model,
    pub status: ModelStatus,
}

/// An immutable point-in-time view of the whole model universe. Readers borrow
/// an `Arc<CatalogSnapshot>` from the watch channel — cloning it is an `Arc`
/// bump, not a deep copy. `BTreeMap` keeps iteration deterministic (stable
/// `models/list` ordering, stable test assertions).
#[derive(Debug, Clone, Default)]
pub struct CatalogSnapshot {
    pub models: BTreeMap<String, LiveModel>,
    /// Bumped on every mutation. A subscriber can tell "did the universe change
    /// since I last looked?" without diffing the map.
    pub generation: u64,
}

impl CatalogSnapshot {
    pub fn get(&self, id: &str) -> Option<&LiveModel> {
        self.models.get(id)
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

/// The single owner of live model state. Holds the `watch::Sender`; hands out
/// receivers via [`subscribe`](ModelCatalog::subscribe). The rich `models/*`
/// commands capture an `Arc<ModelCatalog>` and call its mutators.
#[derive(Debug)]
pub struct ModelCatalog {
    tx: watch::Sender<Arc<CatalogSnapshot>>,
    /// A retained receiver so the channel is never "closed". `watch::Sender::send`
    /// silently DROPS the value when every receiver is gone — a mutation with no
    /// live subscriber would otherwise no-op. Holding this guarantees the live
    /// universe persists every mutation whether or not anyone is watching.
    _keepalive: watch::Receiver<Arc<CatalogSnapshot>>,
}

impl ModelCatalog {
    /// Seed the live universe from the immutable registry. Each model's
    /// [`Availability`] is computed from its provider kind + whether its
    /// resolved GGUF is actually on disk. Generation starts at 0.
    pub fn from_registry(reg: &Registry) -> Self {
        let mut models = BTreeMap::new();
        for m in reg.models() {
            let availability = seed_availability(m, reg);
            models.insert(
                m.id.clone(),
                LiveModel {
                    model: m.clone(),
                    status: ModelStatus {
                        availability,
                        verified: None,
                    },
                },
            );
        }
        let snapshot = CatalogSnapshot {
            models,
            generation: 0,
        };
        let (tx, keepalive) = watch::channel(Arc::new(snapshot));
        Self {
            tx,
            _keepalive: keepalive,
        }
    }

    /// A receiver for live updates. The widget and persona model-selection hold
    /// one of these and react when the universe changes.
    pub fn subscribe(&self) -> watch::Receiver<Arc<CatalogSnapshot>> {
        self.tx.subscribe()
    }

    /// The current snapshot — an `Arc` bump, lock-free. The read path for
    /// `models/list` and friends.
    pub fn snapshot(&self) -> Arc<CatalogSnapshot> {
        self.tx.borrow().clone()
    }

    /// Copy-on-write mutation: clone the current snapshot, bump its generation,
    /// apply `f`, publish a fresh `Arc`. Every mutator below routes through this
    /// so the generation bump and the publish can't be forgotten.
    fn mutate(&self, f: impl FnOnce(&mut CatalogSnapshot)) {
        let mut next = (**self.tx.borrow()).clone();
        next.generation += 1;
        f(&mut next);
        // `_keepalive` guarantees a live receiver, so send() stores the value and
        // cannot fail from a closed channel. The Result is still surfaced as `_`
        // for the type, but the keepalive is what makes the mutation durable.
        let _ = self.tx.send(Arc::new(next));
    }

    /// Add (or replace) a model in the live universe — the path a freshly
    /// discovered provider model or a freshly forged GGUF takes to become
    /// usable without a reboot.
    pub fn register(&self, model: Model, status: ModelStatus) {
        self.mutate(|snap| {
            snap.models
                .insert(model.id.clone(), LiveModel { model, status });
        });
    }

    /// Flip a model to [`Availability::Ready`] after its artifact lands on disk
    /// (`models/pull`). Returns whether the model was present.
    pub fn mark_ready(&self, id: &str) -> bool {
        let present = self.tx.borrow().models.contains_key(id);
        if present {
            self.mutate(|snap| {
                if let Some(live) = snap.models.get_mut(id) {
                    live.status.availability = Availability::Ready;
                }
            });
        }
        present
    }

    /// Record the artifact a `models/pull` just landed: set the live model's
    /// resolved GGUF path (and the multimodal projector path, if one came down)
    /// AND flip availability to [`Availability::Ready`] in one mutation. This is
    /// the truthful pull result — the live universe now knows not just THAT the
    /// model is ready but exactly WHERE its bytes are, so serving resolves the
    /// path off the snapshot instead of re-scanning disk. Returns whether the
    /// model was present.
    pub fn attach_local_artifact(
        &self,
        id: &str,
        gguf_path: std::path::PathBuf,
        mmproj_path: Option<std::path::PathBuf>,
    ) -> bool {
        let present = self.tx.borrow().models.contains_key(id);
        if present {
            self.mutate(|snap| {
                if let Some(live) = snap.models.get_mut(id) {
                    // ONLY a MAIN-model GGUF may become `gguf_local_path`. A pull can
                    // legitimately fetch a SIDECAR into the same repo — the multimodal
                    // projector (`mmproj-*.gguf`) or the multi-token-prediction draft
                    // head (`mtp-*.gguf`) — and those are not the model.
                    //
                    // This matters because `resolve_gguf` honours an EXPLICIT path
                    // FIRST, before the `is_main_model_gguf` filter that the hint and
                    // model-id paths go through. So attaching a sidecar path here made
                    // `hydrate_artifact_sizes` stamp the SIDECAR's size as the model's
                    // `weights_bytes` — and that field is cached on the row by design
                    // (to keep `stat` off the governor's accounting tick), so the wrong
                    // number then survived every later plan until a process restart.
                    //
                    // MEASURED 2026-09-06 on BigMama: pulling the Qwen3.8-27B MTP draft
                    // (`--quant Q4_0`) after the main weights left the planner reading
                    // 1,680,271,648 bytes (the 1.68 GB draft) for a 18,973,870,432-byte
                    // (18.97 GB) model — an 11x under-count. The planner believed it had
                    // ~17 GB more headroom than it did, granted a WIDER context window
                    // on that basis, and the card sat at 32,086/32,607 MiB = 98.4%. A
                    // fresh core reported 18.97 GB from the same files, which is what
                    // identified the row cache as the carrier.
                    //
                    // Sidecars need no path attached: the projector has its own field,
                    // and the draft head is found by `find_mtp_draft_beside` scanning
                    // the snapshot dir. So the correct action is to leave the main path
                    // alone and still re-hydrate — the hydration below resolves the real
                    // main artifact and stamps the right size.
                    if crate::model_registry::artifacts::is_main_model_gguf(&gguf_path) {
                        live.model.gguf_local_path = Some(gguf_path);
                    }
                    if mmproj_path.is_some() {
                        live.model.mmproj_local_path = mmproj_path;
                    }
                    // A path without its size is a row that forces every later estimate
                    // back to the filesystem. Attach both, in the one mutation.
                    crate::model_registry::artifacts::hydrate_artifact_sizes(&mut live.model);
                    live.status.availability = Availability::Ready;
                }
            });
        }
        present
    }

    /// The exact inverse of [`attach_local_artifact`](Self::attach_local_artifact):
    /// after `models/remove` deletes the bytes from disk, forget where they were
    /// and flip the model back to [`Availability::NotDownloaded`] in one mutation.
    /// The live universe now truthfully reports the model as re-acquirable (its
    /// `gguf_local_path`/`mmproj_local_path` cleared), the snapshot generation
    /// bumps, and serving — which plans off the snapshot — stops treating it as a
    /// candidate WITHOUT a reboot. Allocation and deallocation are symmetric: a
    /// pull sets the path + flips Ready; a remove clears the path + flips
    /// NotDownloaded. Returns whether the model was present.
    pub fn detach_local_artifact(&self, id: &str) -> bool {
        let present = self.tx.borrow().models.contains_key(id);
        if present {
            self.mutate(|snap| {
                if let Some(live) = snap.models.get_mut(id) {
                    live.model.gguf_local_path = None;
                    live.model.mmproj_local_path = None;
                    live.model.weights_bytes = None;
                    live.model.mmproj_bytes = None;
                    live.status.availability = Availability::NotDownloaded;
                }
            });
        }
        present
    }

    /// Attach a verification result after `models/try` runs. Returns whether the
    /// model was present.
    pub fn attach_verification(&self, id: &str, report: VerifyReport) -> bool {
        let present = self.tx.borrow().models.contains_key(id);
        if present {
            self.mutate(|snap| {
                if let Some(live) = snap.models.get_mut(id) {
                    live.status.verified = Some(report);
                }
            });
        }
        present
    }
}

/// Compute a model's boot [`Availability`] from its provider kind and, for local
/// models, whether the resolved GGUF is actually on disk. A cloud model is
/// `Ready` (nothing to fetch); a local model is `Ready` only once its artifact
/// exists, else `NotDownloaded` (the `models/pull` target).
fn seed_availability(model: &Model, reg: &Registry) -> Availability {
    let kind = reg
        .provider(&model.provider)
        .map(|p| p.kind)
        .unwrap_or_default();
    match kind {
        ProviderKind::Cloud => Availability::Ready,
        ProviderKind::Local => {
            let on_disk = model
                .gguf_local_path
                .as_ref()
                .map(|p| p.exists())
                .unwrap_or(false);
            if on_disk {
                Availability::Ready
            } else {
                Availability::NotDownloaded
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::catalog;

    // what this catches: the live catalog must seed one LiveModel per registry
    // model with generation 0, and a cloud model (Sonnet) must seed Ready
    // (nothing to download) — proving provenance flows seed→snapshot and the
    // availability rule keys off provider kind, not a name guess.
    #[test]
    fn seeds_from_registry_with_cloud_models_ready_at_generation_zero() {
        let reg = catalog::registry().expect("Rust catalog must validate");
        let catalog = ModelCatalog::from_registry(&reg);
        let snap = catalog.snapshot();

        assert_eq!(snap.generation, 0);
        assert_eq!(snap.len(), reg.models().count());

        let sonnet = snap
            .get("claude-sonnet-4-5-20250929")
            .expect("Sonnet seeded into the live universe");
        assert_eq!(
            sonnet.status.availability,
            Availability::Ready,
            "a cloud model has no artifact to fetch — Ready at seed"
        );
        assert!(
            sonnet.status.verified.is_none(),
            "no verification until models/try runs"
        );
    }

    // what this catches: a mutation must be copy-on-write + observable — the
    // generation bumps, a prior subscriber sees the change, and the registered
    // model appears. This is the "no reboot" contract: add a model at runtime,
    // readers react.
    #[test]
    fn register_bumps_generation_and_is_observable_by_subscribers() {
        let reg = catalog::registry().expect("Rust catalog must validate");
        let catalog = ModelCatalog::from_registry(&reg);
        let rx = catalog.subscribe();
        let before = catalog.snapshot().generation;

        let mut newcomer = reg
            .models()
            .next()
            .expect("catalog has a model to clone")
            .clone();
        newcomer.id = "runtime/freshly-forged-GGUF".to_string();
        catalog.register(
            newcomer,
            ModelStatus {
                availability: Availability::Ready,
                verified: None,
            },
        );

        let after = rx.borrow();
        assert_eq!(after.generation, before + 1, "mutation bumps generation");
        assert!(
            after.get("runtime/freshly-forged-GGUF").is_some(),
            "a model registered at runtime is live without a reboot"
        );
    }

    // what this catches: mark_ready / attach_verification must report presence
    // honestly (no silent success on an unknown id) and actually flip the live
    // status when the model exists.
    #[test]
    fn status_mutators_report_presence_and_apply() {
        let reg = catalog::registry().expect("Rust catalog must validate");
        let catalog = ModelCatalog::from_registry(&reg);

        assert!(
            !catalog.mark_ready("nope/not-a-model"),
            "an unknown id must report false, not a silent success"
        );

        let id = reg.models().next().expect("a model").id.clone();
        assert!(catalog.attach_verification(
            &id,
            VerifyReport {
                text_ok: true,
                vision_ok: None,
                measured_tps: Some(42.0),
                detail: "smoke ok".into(),
            },
        ));
        let snap = catalog.snapshot();
        let live = snap.get(&id).expect("model still present");
        assert_eq!(
            live.status.verified.as_ref().unwrap().measured_tps,
            Some(42.0)
        );
    }

    // what this catches: a models/pull result is recorded truthfully — the live
    // model gets its resolved GGUF + projector paths AND flips to Ready in one
    // generation bump, so serving reads the path off the snapshot instead of
    // re-scanning disk. An unknown id reports false (no silent success).
    #[test]
    // what this catches: a SIDECAR pull clobbering the model's main-weights path,
    // and with it the cached `weights_bytes` every capacity estimate reads.
    // Regression for the 2026-09-06 BigMama measurement — pulling the Qwen3.8-27B
    // MTP draft (`models/pull --quant Q4_0`) after the main weights made the planner
    // read 1,680,271,648 bytes (the 1.68 GB draft) for an 18,973,870,432-byte model.
    // `resolve_gguf` honours an EXPLICIT path FIRST, before the `is_main_model_gguf`
    // filter the hint path goes through, so the sidecar's size was stamped onto the
    // row — and `weights_bytes` is cached there by design, so it survived every plan
    // until a process restart. Consequence was not cosmetic: an 11x under-count
    // granted a wider context window and left the card at 98.4% VRAM.
    #[test]
    fn a_sidecar_pull_never_becomes_the_models_main_weights_path() {
        use std::path::PathBuf;
        let reg = catalog::registry().expect("Rust catalog must validate");
        let catalog = ModelCatalog::from_registry(&reg);
        let id = catalog
            .snapshot()
            .models
            .values()
            .find(|m| m.status.availability == Availability::NotDownloaded)
            .map(|m| m.model.id.clone())
            .expect("seeded universe has a not-downloaded local model");

        // The main artifact lands first, as a real pull does.
        let main = PathBuf::from("/tmp/Qwen3.8-27B-Q4_K_M.gguf");
        assert!(catalog.attach_local_artifact(&id, main.clone(), None));
        assert_eq!(
            catalog.snapshot().get(&id).unwrap().model.gguf_local_path,
            Some(main.clone()),
            "the main artifact IS the model's weights path"
        );

        // Then the MTP draft head — a sidecar in the SAME repo, which is exactly how
        // ggml-org ships Qwen3.8 and exactly what `--quant Q4_0` fetches.
        assert!(catalog.attach_local_artifact(
            &id,
            PathBuf::from("/tmp/mtp-Qwen3.8-27B-Q4_0.gguf"),
            None
        ));
        assert_eq!(
            catalog.snapshot().get(&id).unwrap().model.gguf_local_path,
            Some(main.clone()),
            "an mtp- draft head must NOT replace the main weights path"
        );

        // And the multimodal projector, the other sidecar in that repo.
        assert!(catalog.attach_local_artifact(
            &id,
            PathBuf::from("/tmp/mmproj-Qwen3.8-27B-BF16.gguf"),
            None
        ));
        assert_eq!(
            catalog.snapshot().get(&id).unwrap().model.gguf_local_path,
            Some(main),
            "an mmproj projector must NOT replace the main weights path"
        );
    }

    fn attach_local_artifact_records_paths_and_flips_ready() {
        use std::path::PathBuf;
        let reg = catalog::registry().expect("Rust catalog must validate");
        let catalog = ModelCatalog::from_registry(&reg);

        assert!(
            !catalog.attach_local_artifact("nope/not-a-model", PathBuf::from("/x.gguf"), None),
            "an unknown id must report false, not silently succeed"
        );

        // A local model that seeds NotDownloaded (no artifact on disk).
        let id = catalog
            .snapshot()
            .models
            .values()
            .find(|m| m.status.availability == Availability::NotDownloaded)
            .map(|m| m.model.id.clone())
            .expect("seeded universe has a not-downloaded local model");
        let gen_before = catalog.snapshot().generation;

        let gguf = PathBuf::from("/tmp/pulled-model-Q4_K_M.gguf");
        let mmproj = PathBuf::from("/tmp/mmproj-f16.gguf");
        assert!(catalog.attach_local_artifact(&id, gguf.clone(), Some(mmproj.clone())));

        let snap = catalog.snapshot();
        let live = snap.get(&id).expect("model still present");
        assert_eq!(
            live.status.availability,
            Availability::Ready,
            "pull flips Ready"
        );
        assert_eq!(live.model.gguf_local_path.as_ref(), Some(&gguf));
        assert_eq!(live.model.mmproj_local_path.as_ref(), Some(&mmproj));
        assert!(
            snap.generation > gen_before,
            "the mutation bumps generation"
        );
    }

    // what this catches: detach is the exact inverse of attach — after a
    // models/remove frees the bytes, the live model forgets both paths AND flips
    // back to NotDownloaded in one generation bump, so serving stops seeing it as
    // a candidate. Round-tripping attach→detach returns the entry to its
    // pre-pull shape. An unknown id reports false (no silent success).
    #[test]
    fn detach_local_artifact_is_the_inverse_of_attach() {
        use std::path::PathBuf;
        let reg = catalog::registry().expect("Rust catalog must validate");
        let catalog = ModelCatalog::from_registry(&reg);

        assert!(
            !catalog.detach_local_artifact("nope/not-a-model"),
            "an unknown id must report false, not silently succeed"
        );

        let id = catalog
            .snapshot()
            .models
            .values()
            .find(|m| m.status.availability == Availability::NotDownloaded)
            .map(|m| m.model.id.clone())
            .expect("seeded universe has a not-downloaded local model");

        // Allocate (pull), then deallocate (remove).
        catalog.attach_local_artifact(
            &id,
            PathBuf::from("/tmp/pulled-Q4_K_M.gguf"),
            Some(PathBuf::from("/tmp/mmproj-f16.gguf")),
        );
        assert_eq!(
            catalog.snapshot().get(&id).unwrap().status.availability,
            Availability::Ready
        );
        let gen_after_attach = catalog.snapshot().generation;

        assert!(catalog.detach_local_artifact(&id));
        let snap = catalog.snapshot();
        let live = snap.get(&id).expect("model still present");
        assert_eq!(
            live.status.availability,
            Availability::NotDownloaded,
            "remove flips NotDownloaded — the model is re-acquirable again"
        );
        assert!(
            live.model.gguf_local_path.is_none(),
            "the gguf path is forgotten"
        );
        assert!(
            live.model.mmproj_local_path.is_none(),
            "the projector path is forgotten"
        );
        assert!(
            snap.generation > gen_after_attach,
            "the deallocation also bumps generation"
        );
    }
}
