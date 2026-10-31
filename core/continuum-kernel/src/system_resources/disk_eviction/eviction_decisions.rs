#![cfg(test)]
//! Every cache class's DECIDED eviction story: an owner pool that evicts it, or a deferral
//! naming the task that owns it. Data, read by
//! `tests::every_cache_class_has_a_decided_eviction_story`, which fails on a class in
//! `standard_tracked_dirs` with neither. Its own file so the decisions are one list to read
//! and extend (card 573cc58a: it was 230 lines inside a 1,144-line test module).

/// Classes with an OWNER pool that really evicts them.
pub(super) const OWNED: &[&str] = &[
        "cargo-target",
        // Same owner and the same rule as the shared target: derived build output,
        // re-creatable by definition, evicted oldest-artifact-first under pressure.
        "cargo-target-wt",
        // Same owner, same loop, same reasoning — the airc build tree is derived
        // artifacts like its two siblings, so it is OWNED rather than deferred.
        "cargo-target-airc",
        // Owner: CitizenWorkspacePool — dormant, non-resident WORKSPACES only, and
        // only after complete verified preservation. Memory is never evicted.
        "citizens",
        "genome-models",
        "logs",
        "probes",
        // Owner: perception::eye_reaper. A profile whose browser is gone is dead
        // weight the instant the browser dies, so eviction is not age-based — the
        // reaper removes every profile no live process names, at boot and on the
        // pressure edge.
        "eye-profiles",
];

/// Classes whose eviction is deferred, each with the reason and the task that owns it.
pub(super) const DEFERRED: &[(&str, &str)] = &[
        // Registered by card 75dd8bfe. Each writer now bounds its own files per persona
        // (prompt captures: two 64 MiB segments; workspace traces: 2 x MAX_LOG_BYTES), but
        // the class total scales with persona count, so the broker needs an owner it can
        // lower. Deferred, not owned, until that pool exists: a live sink's file must
        // never be deleted beneath it.
        (
            "fixtures",
            "6fc44bda: FixturesPool — evict rotated capture generations (*.jsonl.1, \
             *.prev.jsonl/.prev.pidx) oldest-first across personas, then non-resident \
             personas' live files; never a file a live sink holds",
        ),
        // Registered 2026-09-28 with claim-time cloning: a repo card for a repo the
        // node never checked out is cloned here so the citizen's hands root in it.
        // Deferred, not owned, because blind deletion is UNSAFE: every live per-card
        // worktree is a linked worktree of one of these clones and dies with it.
        (
            "repos",
            "5c5f3b42: managed-clone pool — evict a clone no live card worktree is cut \
             from and no claimed card names, oldest first, retiring its repo_registry \
             entry in the same step; sweep `.partial-` dirs a timed-out clone left",
        ),
        (
            "hf-hub",
            "#155: hub LRU keyed on last-access — downloads are re-fetchable. Measured \
             2026-09-06 (M5): 51 GB of `models--*` blobs, every one a staging copy of a \
             GGUF already promoted into the `models` store; the owner evicts a hub entry \
             the moment its artifact is present in the store, and the rest by last access",
        ),
        // Registered 2026-09-06, the day the M5 hit zero free with 360 GB here and the
        // governor unable to see it. Two sub-classes: SERVED weights (the catalog's
        // active/pinned/roster tiers — never blind-deleted, same rule as genome-models'
        // NvmeServingTierPool) and UNROSTERED weights (a store entry matching no catalog
        // id: two concluded experiments held 186 GB that day). The owner evicts the
        // second class oldest-access first and must ask the registry resolver, never a
        // name heuristic, whether an entry is on a serving path.
        (
            "models",
            "58c27b0c/#155: unrostered-weights pool — evict store entries the registry \
             resolves to NO catalog id and no active/pinned lane, oldest access first; \
             served tiers are the NvmeServingTierPool's, never this pool's",
        ),
        // Registered 2026-09-18, the day the 5090's system volume hit 0 bytes free
        // with 760 GB of K3 expert bank sitting on it unweighed. The bank is a SERVING
        // artifact when K3 is the served model (per-token paged off NVMe — never the
        // cold HDD tier, [[hdd-vs-nvme-is-a-residency-tier]]) and a COLD artifact the
        // rest of the time. The owner is the NvmeServingTierPool's migrate rule
        // extended to this class: not on a serving path → migrate to the cold tier
        // (`<cold>/continuum-cold/k3-container`), never blind-delete; on a serving
        // path → protected by `serving_active_artifacts`, exactly like a served GGUF.
        (
            "k3-container",
            "a5d4c876/#155: cold-tier MIGRATION when K3 is not the served model, through \
             NvmeServingTierPool's migrate rule with the container as one artifact; \
             protected by serving_active_artifacts while served; never deleted in place",
        ),
        ("core-artifacts-configured", "139c651d: shared configured staging root; writer projected-byte admission and post-success retention, no migration/deletion of legacy cache"),
        ("core-artifacts-legacy", "4dedb177: the historical ~/.continuum/cache/artifacts after $CONTINUUM_HOME moved; no writer adds to it, tracked only while it exists so its bytes stay visible; no automatic deletion"),
        (
            "core-artifacts",
            "50ca737e: bounded by its writer — the deploy consumer keeps the newest 3 \
             CI-built cores and removes older ones only after successful preparation; all \
             re-downloadable from the canary releases. Owner when built: a capped pool \
             like cargo-target, if pressure ever needs more than three",
        ),
        (
            "moe-probe",
            "a5d4c876/#155: age-based sweep — every run is a re-creatable measurement \
             (expert-activation traces, coverage curves); the K3 predictor memories hold \
             the conclusions. Owner when built: a capped appender like the log pool",
        ),
        (
            "eval-captures",
            "#155: age-based sweep — every file is a re-creatable diagnostic (kv-diag \
             snapshots, wire-request jsonl from SERVING_WIRE_CAPTURE_DIR); writers are \
             opt-in and quiet by default, so the class grows only while an operator is \
             actively hunting. Owner when built: a capped appender like the log pool",
        ),
        // Measured 2026-09-06 (M5, the day the disk hit zero): 33 workspace copies of the
        // repo (12 resident citizens, 21 dormant), 7–20 GB EACH, none of it hers: a 7.6 GB
        // .git holding the canonical clone's history plus COPIED stale worktree metadata
        // (5 × 462 MB of vendored llama.cpp packs per copy), 4.5 GB of gitignored model
        // caches (tools/models), 2 GB of mobile build output, node_modules. The by-hand
        // eviction that day (alternates to the canonical objects + repack -l, ignored
        // dirs dropped, dormant copies patch-then-dropped) is the pool's spec.
        // Sibling of `citizens` and inherits its rule: a LIVE mind's longterm.db and
        // working-set.json are MEMORY, never auto-evicted. What IS evictable is the
        // GHOST sub-class — a dir whose uuid appears in no roster and which never
        // recorded a turn, left by the spawn name-pool (#437). Measured 2026-08-20:
        // 295 dirs, 286 of them under 100 KB, 2 real citizens. Small in bytes, which is
        // exactly why it went unnoticed for so long — the hazard here is not capacity,
        // it is that ghost identities pollute the roster and the demand ceiling. An
        // owner pool must key on "has this uuid ever completed a turn", never on size.
        (
            "personas",
            "#155/#437: per-persona MEMORY, never blind-LRU'd. Evictable sub-class is \
             GHOST dirs only — no roster entry AND no recorded turn — which needs a pool \
             that can ask the roster, not a size heuristic",
        ),
        // Joel 2026-10-19: the retired harness's state is ARCHIVED, never deleted. Written
        // once at boot and never grown, so it has no pressure story to defer to a pool.
        (
            "benchmark-harness-archive",
            "c4f4b8cd: never evicted by the substrate — Joel decided to archive, not delete; \
             removal is an operator's decision. Bounded: written once, the receipt stops \
             any second write",
        ),
        // Registered 2026-10-29 (the M5 at ENOSPC, 448 GB here). WorktreeTargetsPool is
        // built and registered in OBSERVE mode: it measures, reports pressure and publishes
        // what keep-recent or a hot volume would take, and deletes nothing. Deferred, not
        // owned, because agents build here from shells the disk registry never admitted, so
        // its exclusion cannot protect their work (Codex on #5601).
        (
            "wt-targets",
            "#5601 next slice (Codex): build admission, every build into a worktree target \
             admitted through the disk registry with a lease; then WorktreeTargetsPool runs \
             in WorktreeEvictionMode::Delete (keep recent within budget, oldest unheld first, \
             a hot volume may take the newest unheld)",
        ),
        ("forge", "#155: export trimmer — intermediates only, published artifacts stay"),
        // Registered the day benchmark/swe-* landed, BEFORE a sweep ran. Everything under
        // it is re-creatable — repo clones from git, venvs from uv, the dataset from HF —
        // so eviction is safe by construction; what it must never do is delete an instance
        // dir mid-grade, which is why it wants a pool that knows the in-flight set rather
        // than a blind LRU. Until then it is tracked and REPORTED, so the class can never
        // be the silent one again.
        (
            "benchmarks",
            "#155: LRU over per-instance dirs, skipping the in-flight set — clones/venvs \
             are re-creatable from git+uv, so only an active grade is at risk. NOT \
             everything under it is re-creatable: `swe/captures/run-*/attempt-N.patch` \
             is a citizen's actual diff, deleted from her workspace the moment the next \
             attempt resets it (#379). An eviction pool here must treat captures as \
             EVIDENCE — small, and the only thing that can answer what she wrote — and \
             reclaim the bulky re-creatable clones/venvs instead. Corrected 2026-08-18: \
             this entry read \"everything under it is re-creatable\", which the 25 \
             patches already sitting there had falsified since before it was written. \
             The `swe/grades` sub-class is owned in-file since 2026-09-25: \
             `GradeCheckoutHold` removes a grade's tree when the grade ends and the \
             next grade sweeps any tree no live grade holds",
        ),
        // Steady-state owner ALREADY EXISTS in-file: RAII drop on every in-process
        // return path + the provision-time orphan sweep for worlds a killed process
        // leaves behind (an eval run cannot survive its process, so any non-live
        // sibling is debris; everything inside is a CoW clone of the checkout —
        // re-creatable by construction). Deferred only for the broker seam: under
        // real disk pressure the broker cannot yet claw these bytes back BETWEEN
        // provisions — that wants a pool that consults `live_eval_roots()`, never
        // a blind LRU that could delete a mid-exam world.
        (
            "eval-roots",
            "#155: broker-reachable pool over cognition/eval::live_eval_roots(); \
             sweep + RAII already own the steady state and the crash path",
        ),
        // Steady-state owner ALREADY EXISTS on every save:
        // `inference::slots::trim_page_store_after_save` keeps the model's one
        // page dir under its byte budget, oldest pages first and never one
        // younger than a day, and each activity owns ONE file that saves
        // overwrite in place (count bounded by residents × rooms). Legacy
        // per-window dirs (`<model>--c<step>`) are no longer written; the newest
        // moves into the model's dir once, the rest wait for an operator.
        // Deferred only for the broker seam: under real disk pressure the broker
        // cannot yet claw back live pages — that wants a pool that asks the slot
        // pool which activities are resident (their pages are one eviction from
        // being needed) versus departed, never a blind LRU.
        // airc's per-room projection snapshots (operator scope; the personas' live under
        // `citizens`). Size is O(rooms × projection): the board snapshot is the folded
        // board (cards, not events), the wall snapshot is the room's posts (few). Both
        // are accelerators rebuilt from the transcript on any anomaly, so eviction is
        // always safe; what they want is a sweep keyed on the room directory — a
        // snapshot for a room that is archived or no longer subscribed is dead weight.
        (
            "airc-board-cache",
            "1291173d/#155: sweep snapshots of rooms absent from the directory or \
             archived; rebuilt from the transcript on the next read, so deletion is \
             always safe",
        ),
        (
            "airc-wall-cache",
            "airc#1390/#155: same sweep as airc-board-cache — one file per room, the \
             room's posts; rebuilt on the next read",
        ),
        ("airc-mirrors", "card d166e4f2 (Cormac): never pruned in place (preciousObjects; card clones borrow its objects); evict a whole mirror only when no live card clone's alternates name it. Owner not built"),
        // airc's own state (card 0af5d5ec). Deferred, never owned by a continuum
        // delete: every byte's safety is a fact only airc holds — which worktree
        // carries unpushed work, which store pages are free, which log the daemon
        // still writes. Each owner drains through airc.
        (
            "airc-worktrees",
            "186ee967: AircWorktreePool — the broker runs airc's own drain (`airc work \
             cleanup --force`: Merged/Closed cards in every subscribed room, airc#1574), \
             never a dirty or unpushed worktree, never a continuum rm",
        ),
        (
            "airc-store",
            "airc bdf34c4c: retention reclaims the pages it frees (the pass removes rows \
             but reports `reclaim None`); the event store is never deleted from outside",
        ),
        (
            "airc-daemon-log",
            "airc 551fe5ae: the daemon bounds its own log (rotate at a cap, one \
             predecessor) and writes it in its own home, not the spawning scope's",
        ),
        (
            "kv-pages",
            "#155: broker-reachable pool keyed on slot-pool residency; the \
             spawn-time generation sweep already owns the stale-geometry path",
        ),
        // Every lane's --train-dir (2026-09-26). A finished adapter is MOVED into its
        // job dir by the engine trainer, so what stays is a run in flight (never
        // touched) or a crashed run's partial file (dead the moment its job is).
        (
            "engine-train",
            "charter S3/S6 engine trainer: delete every `<uuid>.gguf` whose job is not \
             live on the TrainingJobBoard; a live job's file is never touched",
        ),
];
