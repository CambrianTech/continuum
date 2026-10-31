//! The substrate's registered disk classes: one row per directory it fills, each with an
//! eviction decision (`disk_eviction`). A new class is a row here, beside its writer's migration
//! onto [`super::ClassWriter`].

use super::*;

/// The substrate's known cache classes, rooted under `home` (normally the
/// user's home directory). These are exactly the directories that produced
/// the 2026-07-13 creep, each of which needs an eviction owner (wire 2):
///
/// | class | grows by | owner (planned) |
/// |---|---|---|
/// | cargo-target | every cargo build/test | age-based sweeper |
/// | genome-models | model downloads/forges | model-store LRU |
/// | hf-hub | HF downloads | hub LRU |
/// | citizens | persona workspaces/stores | WorkspaceResolver CoW fix |
/// | forge | export intermediates | export trimmer |
///
/// Paths that the operator can RELOCATE are resolved through the same authority the rest of the
/// substrate uses, never re-derived from `home`. A tracked dir that points somewhere the artifacts
/// are not is worse than no tracking at all: the scanner reports a reassuring 0 bytes, the class
/// looks governed, and the real bytes accumulate unwatched and unowned by any eviction pool —
/// which is precisely the 2026-07-13 shape this registry exists to prevent, one level up.
///
/// MEASURED on BigMama 2026-08-05: `hf-hub` tracked `~/.cache/huggingface` (0 bytes) while the
/// cold-storage installer had pointed `HF_HOME` at `D:\continuum-cold\huggingface` holding
/// **2.48 TB** — Kimi-K3 at 1.45 TB, Kimi-K2.7 at 430 GB, GLM-5.2 at 375 GB. Entirely invisible to
/// the disk governor.
///
/// `home` is the user's home, where airc keeps its own state. `continuum` is the continuum home,
/// resolved once by `paths::continuum_home` (card 4dedb177), so `$CONTINUUM_HOME` and a test's
/// home hold for every continuum class instead of a hand-built `~/.continuum`.
pub fn standard_tracked_dirs(home: &std::path::Path, continuum: &std::path::Path) -> Vec<Arc<TrackedDir>> {
    // The HF cache is relocatable (HF_HOME / config.env, set by the cold-storage installer), so ask
    // the resolver instead of assuming the default. `huggingface_cache_root()` returns `<root>/hub`;
    // track the PARENT so the class covers the whole cache (blobs, snapshots, locks, refs), which is
    // what actually consumes the volume.
    // NOT `.unwrap_or_else(|| home.join(".cache/huggingface"))`. That fallback is a FABRICATION in
    // the sense swallow-audit.py means: it substitutes a plausible path for an unknown one, and the
    // scanner then reports a confident 0 bytes for a class whose real bytes are elsewhere. That is
    // the exact defect this function was changed to fix — 2.48 TB invisible because the tracked path
    // was not the real one — so silently re-introducing it as the failure branch would be the same
    // bug wearing a fallback's clothes.
    //
    // If the resolver cannot answer, the honest outcome is to say so and NOT claim the class is
    // tracked. A missing hf-hub row is visibly missing; a wrong one looks governed.
    let hf_root = match crate::paths::huggingface::huggingface_cache_root()
        .and_then(|hub| hub.parent().map(|p| p.to_path_buf()))
    {
        Some(root) => Some(root),
        None => {
            tracing::warn!(
                probe_class = "disk.tracked_dirs",
                "hf-hub cache root could not be resolved (no HF_HOME, no config.env entry, no home \
                 dir) — the hf-hub class is NOT being tracked this boot. Not defaulting to \
                 ~/.cache/huggingface: a tracked dir pointing at the wrong place reports a \
                 reassuring zero and is worse than an absent one."
            );
            None
        }
    };

    let mut dirs = vec![
        // Retain the historical root when configuration moves future writes.
        // Boot and later writers register configured roots in this same registry.
        cached(continuum, "cargo-target"),
        // THE CACHE I INVENTED IS A CACHE THE SUBSTRATE OWNS (Joel, 2026-09-08: "the
        // system is supposed to manage disk; you've been defying design principles so it
        // can't"). Worktree builds must not write into the shared target — a worktree
        // build once latched its CARGO_MANIFEST_DIR into it and broke the main checkout
        // (card d2cda466) — so every agent building in a worktree exports
        // CARGO_TARGET_DIR=cargo-target-wt. That directory reached 87 GB unseen, because
        // it was never registered: the 2026-07-13 law says a new directory the substrate
        // writes unbounded data into gets a TrackedDir row and an eviction decision, and
        // hand-sweeping it is exactly the compensation the law exists to end.
        cached(continuum, "cargo-target-wt"),
        // THE THIRD CARGO TREE, unregistered until now (2026-09-21, the M5 at 97%: this
        // directory held 9.8 GB that no reporter could see and no pool could evict). The
        // airc build gets its own target dir for the same reason worktree builds do —
        // sharing one target across differently-configured builds is what card d2cda466
        // was — and the row was simply never added when it appeared. Identical class to
        // its two siblings: derived artifacts, re-creatable by definition, so it takes
        // the SAME `CargoTargetPool` owner by being named in the registration loop
        // (`ipc/mod.rs`) rather than getting a second pool type.
        //
        // This is the 2026-07-13 law's own failure mode repeating: `cargo-target-wt`
        // reached 87 GB unseen because it had no row, the law was written, and then the
        // next cargo tree to appear went unregistered anyway. A row is not remembered by
        // a doc; it is remembered by the class-coverage test below refusing an unlisted
        // class, which is why this lands with its eviction entry in the same change.
        cached(continuum, "cargo-target-airc"),
        // The eye's browser profiles. Playwright defaults them into the OS temp dir,
        // where nothing of ours can see or evict them: 337 leaked profiles from 31
        // orphaned browsers accumulated there over a week (card de9b8876). A directory
        // under our own cache root is one the monitor can weigh and the reaper can clear.
        cached(continuum, "eye-profiles"),
        // CI-built cores the deploy consumer downloads instead of compiling (card 50ca737e):
        // Legacy placement remains observable after configured storage redirects future writes.
        TrackedDir::new("core-artifacts", continuum.join("cache/artifacts")),
        // The served-weights store. UNTRACKED until 2026-09-06 — the largest class on the
        // volume (360 GB on the M5 that day: Flash-Next 123, Kimi-Linear 95, DeepSeek-V4-Flash
        // 91, Qwen3.8-27B 20, Ornith 20) and invisible to both halves of the governed-disk
        // contract while the disk went from 46 GB free to ZERO under a test build. 186 GB of
        // it was weights on NO serving path (two concluded experiments), which is exactly the
        // sub-class an owner evicts: not in the catalog, not the active or pinned model.
        kept(continuum, "models"),
        TrackedDir::new("genome-models", continuum.join("genome/models")),
        kept(continuum, "citizens"),
        // Per-persona durable mind state: longterm.db + working-set.json, one dir per uuid.
        // UNTRACKED until 2026-08-20, which made it invisible to BOTH halves of the governed-
        // disk contract — no size report, and the eviction guard could not flag a missing
        // decision because that guard iterates THIS list. Exactly the silent-class shape the
        // 2026-07-13 incident was about, found at 295 dirs / 18 MB with 286 of them under
        // 100 KB — spawn ghosts (#437), not minds.
        kept(continuum, "personas"),
        kept(continuum, "forge"),
        // Benchmark working set: per-instance repo clones + per-instance venvs. Grows LINEARLY
        // with instances graded — a full SWE-bench Lite sweep is 300 repos, and one sympy
        // checkout is ~240 MB. Entirely re-creatable (git + uv), which is what makes it a
        // cache class rather than data.
        kept(continuum, "benchmarks"),
        // The retired benchmark harness's state, moved here once at boot (card c4f4b8cd
        // step 2; `harness_archive`). Written once, never grown.
        TrackedDir::new("benchmark-harness-archive", continuum.join("archive/benchmark-harness")),
        // Managed clones of repos a citizen claimed a card for when this node had no
        // checkout (`card_staging::ensure_managed_clone`). One full clone per repo, and
        // every per-card worktree for that repo is cut from it.
        kept(continuum, "repos"),
        // THE LARGEST THING ON THE 5090'S SYSTEM VOLUME WAS INVISIBLE (2026-09-18, the
        // day it hit 0 bytes free): the Kimi K3 expert-bank container, 760 GB, plus
        // 25 GB of MoE probe runs — both written under the home by the K3 pager work
        // and registered nowhere, so the daemon weighed 200 GB of caches on a 1.9 TB
        // volume that was 1.8 TB used and could not say where the rest went. Same law
        // as every row above: a directory the substrate writes unbounded data into gets
        // a row and an eviction decision (deferred, named, in `disk_eviction.rs`).
        TrackedDir::new("k3-container", continuum.join("kimi-k3-container")),
        // The cognition capture glass box: per-persona prompt captures and workspace traces
        // (and the replay fixtures beside them). UNTRACKED until card 75dd8bfe, the day the
        // 5090's C: reached 98% with a 3.0 GB workspace trace in it; IntelMac held 1.3 GB
        // across 76 trace files. Its writers now bound each persona's files; the total still
        // scales with persona count, which is why the class needs this row and an owner.
        kept(continuum, "fixtures"),
        kept(continuum, "moe-probe"),
        // The substrate's OWN rotation-generation dirs. Registered
        // 2026-08-06 — they had been the two directories continuum
        // writes to most continuously and the only ones the disk
        // monitor could not see, because the writer
        // (`routing::capped_appender`) bounded itself with a private
        // constant and nobody treated that as a governed class. A
        // writer that caps itself is not governance: the broker can't
        // claw bytes back under real disk pressure, and a
        // still-unbounded writer (the probe sink booted through
        // `tracing_init` was exactly that until today) accumulates
        // invisibly. Owner: `RotationLogPool` — see
        // `super::rotation_log_pool`.
        // airc's transcript-projection snapshots for the OPERATOR scope (each persona's ride
        // inside her airc home under `citizens`): the work board's (shipped untracked, card
        // 1291173d) and the wall's (airc#1390, 2026-09-06). One JSON per room; the wall one
        // holds every post on the room, the board one the folded projection. Registered on
        // the pin bump that adopted the wall cache, on the reviewer's point that a cache
        // growing with room depth on 200k–470k-event rooms is exactly the class the
        // 2026-07-13 rule exists to catch before it is the incident.
        TrackedDir::new("airc-board-cache", home.join(".airc/work-board-cache")),
        TrackedDir::new("airc-wall-cache", home.join(".airc/wall-cache")),
        // airc's own durable state on the machine-account scope, unweighed until card
        // 0af5d5ec. Measured IntelMac 2026-10-15T22:28Z: worktrees 103 GB (one per claimed
        // card, never drained), events.sqlite 678 MiB, the daemon log 523 MiB (written into
        // whichever scope spawned the daemon, here the project's `.airc`). airc owns every
        // byte; continuum weighs them so pressure names its biggest holder.
        TrackedDir::new("airc-worktrees", home.join(".airc/worktrees")),
        // File roots with the companions they own: the store's WAL and shared-memory
        // index, the log's one rotated predecessor (airc 551fe5ae).
        TrackedDir::with_companions("airc-store", home.join(".airc/events.sqlite"), &["-wal", "-shm"]),
        TrackedDir::with_companions("airc-daemon-log", home.join(".airc/airc-daemon.log"), &[".1"]),
        // The node's bare repo mirrors that residents' card clones borrow objects from
        // (card d166e4f2). One per repo; it grows with the repo's history and is never
        // pruned (preciousObjects), so its size is the class to watch.
        TrackedDir::new("airc-mirrors", home.join(".airc/mirrors")),
        kept(continuum, "logs"),
        kept(continuum, "probes"),
        // #312 ephemeral exam worlds: one CoW clone of the checkout per eval run
        // (~31 GB logical each). RAII drop removes them on every in-process return
        // path, and the provision-time orphan sweep (`cognition/eval.rs::
        // sweep_orphan_eval_roots`) owns the crash path a SIGTERM'd reboot leaves
        // behind — registered 2026-08-23 after exactly that: a mid-run reboot
        // orphaned a full clone that no reporter could see, the silent-class shape
        // the 2026-07-13 incident was about.
        TrackedDir::new("eval-roots", std::env::temp_dir().join("continuum-eval")),
        // Diagnostic captures: cognition kv-diag snapshots + the wire-request
        // capture (SERVING_WIRE_CAPTURE_DIR default location). Opt-in writers,
        // bounded by operator attention in practice — tracked so "in practice"
        // never becomes the 2026-07-13 silent-class shape.
        kept(continuum, "eval-captures"),
        // KV disk pages (restore economy): one file per activity, overwritten on
        // each save, under one subdir per MODEL (any window restores from it).
        // Bounded by residents × rooms per model, and by the save-side store
        // budget (`inference::slots::trim_page_store_after_save`, oldest pages
        // first, never one younger than a day). A page at 20-40k tokens of q8_0
        // KV is hundreds of MB, so the class is small in COUNT but real in
        // bytes — exactly what must never be an untracked writer.
        cached(continuum, "kv-pages"),
        // Every lane's `--train-dir`: adapters the engine's /train writes, in flight or
        // left by a crashed run (a finished one moves into its job dir). Registered with
        // the flag itself (2026-09-26) so it is never an unweighed writer.
        cached(continuum, "engine-train"),
    ];
    // Present only when its real location is KNOWN (see the warn above). Kept
    // CONDITIONAL rather than defaulted: fabricating a path here is how a class
    // gets "tracked" at a location nothing writes to, which reads as an empty
    // cache forever.
    // Preserve the historical cache; relocation changes future writes only.
    match crate::paths::artifacts::root_under(continuum) {
        Ok(root) if root != continuum.join("cache/artifacts") => {
            dirs.push(TrackedDir::new("core-artifacts-configured", root));
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(probe_class = "disk.tracked_dirs", %error, "configured artifact root unavailable"),
    }
    // Artifacts written before the home moved stay where they were written: tracked, once, while
    // they exist, so relocating `$CONTINUUM_HOME` never hides bytes already on disk (card 4dedb177).
    let legacy = crate::paths::artifacts::legacy_root(home);
    if legacy.is_dir() && dirs.iter().all(|tracked| tracked.path() != legacy.as_path()) {
        dirs.push(TrackedDir::new("core-artifacts-legacy", legacy));
    }
    if let Some(root) = hf_root {
        dirs.push(TrackedDir::new("hf-hub", root));
    }
    dirs
}

/// A durable class's default root: its name under the continuum home. A row spells a path only
/// where it differs (a relocatable cache, airc's own home, the system temp dir).
fn kept(continuum: &std::path::Path, name: &'static str) -> Arc<TrackedDir> {
    TrackedDir::new(name, continuum.join(name))
}

/// A rebuildable class's default root: its name under `cache/`, where everything the substrate
/// can regenerate lives together.
fn cached(continuum: &std::path::Path, name: &'static str) -> Arc<TrackedDir> {
    TrackedDir::new(name, continuum.join("cache").join(name))
}
