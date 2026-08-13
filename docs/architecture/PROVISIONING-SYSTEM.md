# The Provisioning System — one loader, not a pile of shell scripts

Joel: *"Ideally it's more of a download and prerequisite system itself, loader,
installer — not just a bunch of stupid-ass shell scripts."*

This is the structural replacement for the ad-hoc install/download layer. Everything a
fresh public user needs to reach a running live video call — prerequisites, model
weights, avatars, voices, service registration — is resolved by ONE Rust-owned system
from DATA, verified and resumable, behind a single command. No bespoke script per
artifact type.

## The anti-pattern we're replacing

Today the install/provision layer is a pile of shell scripts, each reinventing the
same primitives with no shared contract:

| Script | Reinvents |
|---|---|
| `install.sh` (repo-root, 1183 L) + `tools/scripts/install.sh` | dep checks, build orchestration — TWO of them, unreconciled |
| `download-models.sh` / `.ts` | HF URL fetch, retry, place-on-disk |
| `download-avatar-models.sh` | the SAME fetch/extract, for VRMs |
| `setup-rust.sh`, `preflight.sh` | prerequisite checks |
| `start-server.sh`, `install-service.sh` | launch + upstart |

They drift (two install.sh), can't be unit-tested, fail in bespoke ways, and none of
them share "is this present? verified? resumable?" logic. A model download and an
avatar download are the same operation wearing different bash.

## The seed that already works: #1871

The avatar catalog (#1871) is the pattern, done right, for ONE artifact type:
- a single-source **catalog** (`AVATAR_CATALOG`) carries `{id, url, source_kind,
  license}` + the derived local path;
- a projection (`avatar-catalog.json`) the downloader reads;
- provisioning fetches what's missing, writes a manifest, **fail-loud** if the source
  is unreachable; the runtime resolves the local path by **deriving** it from the id
  (not a hardcoded path — cf. the model-path unhardcode).

Generalize that one pattern and the whole shell-script pile collapses into it.

## The system

Three Rust traits + one orchestrator, all in the core ([[rust-is-the-core-node-is-the-shell]]).

### 1. `ArtifactSource` — one abstraction for every downloadable thing
```rust
trait ArtifactSource {
    fn kind(&self) -> ArtifactKind;                 // Model | Avatar | Voice | Bin
    fn catalog(&self) -> Vec<ArtifactSpec>;         // data-driven, single source
    fn resolve_local(&self, id: &str) -> Option<PathBuf>; // DERIVE, never hardcode
}
struct ArtifactSpec {
    id: String, url: String, source_kind: SourceKind, // hf-file | zip | direct
    checksum: Option<String>,                          // verify after download
    license: Option<String>,
}
```
Impls: `ModelSource` (over `model_registry::catalog`), `AvatarSource` (over
`AVATAR_CATALOG` — already exists), `VoiceSource`, `BinarySource` (llama-server).
Each is the SINGLE source of truth for its artifact type. The avatar impl is #1871
already.

### 2. `Prerequisite` — a checked, installable dependency
```rust
trait Prerequisite {
    fn name(&self) -> &str;                    // cmake, rust, node, ffmpeg, llama-server
    fn check(&self) -> PrereqState;            // Present(version) | Missing | TooOld
    fn install_hint(&self) -> InstallPlan;     // brew formula / apt pkg / rustup / fail-loud
}
```
A data manifest of prerequisites per platform. `check()` is pure; `install_hint()`
names the platform-native install (never silently, always fail-loud with the exact
remedy — [[fallbacks-are-illegal-fail-loud]]).

### 3. `Downloader` — the ONE fetch primitive
Content-addressed, **resumable** (range requests / partial-file continue), **verified**
(checksum when present), **fail-loud** per-artifact but non-fatal to the batch
(continuum#1087: a flaky CDN for one avatar must not block the rest), structured
progress on the bus ([[observability-as-substrate]]). Written once; every
`ArtifactSource` uses it. Kills the per-script curl/retry/extract copy-paste.

### 4. `Provisioner` — the orchestrator + the single command
```rust
// cu provision  (or the core self-provisions on launch)
fn provision(need: &ProvisionPlan) -> ProvisionReport {
    // 1. prerequisites: check all → fail-loud on missing with the remedy
    // 2. artifacts: for each needed {model per persona/tier, avatar, voice, bin}
    //    → resolve_local; if absent, Downloader.fetch(spec); verify; manifest
    // 3. report: what was present / fetched / failed, per artifact
}
```
`ProvisionPlan` is DEMAND-DRIVEN: it asks "what do THESE personas at THIS tier on THIS
hardware need?" — so a 2-persona laptop provisions a small model, a 14-persona
workstation provisions the teacher + VL model. It composes with the inference-lane
planner (the lane daemon decides *how many* run; the provisioner ensures the weights
are *on disk*).

### 5. The store is a CACHE — the Provisioner owns disk, not just downloads

Disk is finite on the misfit grid (a MacBook Air is not the 16 TB store), so the
artifact store is a **cache**, and the Provisioner OWNS it — it's the disk authority
for weights/avatars/voices, the way the ResourceGovernor owns VRAM ([[resource-authority-is-a-system-concern]]).

- **Pin the needed set** — "we need what we need": the artifacts the *currently
  active* personas + lanes require (the `ProvisionPlan`) are PINNED and guaranteed
  present.
- **Evict the rest** — everything unpinned is evictable cache. When fetching a needed
  artifact would blow the disk budget, evict least-needed-first (LRU / last-used),
  exactly the shape of the genome pager but for the whole on-disk store.
- **`DiskState` is the reasoning primitive** — every `ArtifactSource` reports whether
  an artifact is present + how many bytes, so the cache can compute "reclaimable if I
  evict X, Y" against "need N more bytes for Z".
- **Fail loud when it won't fit** — if the pinned/needed set doesn't fit even after
  evicting everything unpinned, that's a hard truth about this machine (too many big
  models for this disk) — name it, don't silently thrash ([[fallbacks-are-illegal-fail-loud]],
  [[model-fit-is-the-priority-single-machine-first]]).

So `provision(plan)` is really `reconcile(plan, disk_budget)`: ensure the pinned set
is on disk, evicting unpinned cache as needed, refusing loudly if impossible.

## Invariants

1. **Data-driven** — every artifact is a catalog entry, never a hardcoded path or a
   URL buried in bash. Adding a model/avatar/voice is one entry (the #1871 rule).
2. **Derive, don't hardcode** — local paths derive from the id (cf. the model-path
   unhardcode); the catalog declares the *source*, the resolver finds the *file*.
3. **Fail loud, batch-resilient** — a missing prerequisite names its exact remedy; a
   flaky per-artifact download logs + continues, never silently "works."
4. **Resumable + verified** — a 9 GB model download survives an interrupt; a corrupt
   file is caught by checksum, not served as a broken brain.
5. **One command** — `uu provision` (and the core self-provisions on launch); the
   shell scripts become thin shims that call it, then get deleted.
6. **Testable** — every piece is a Rust unit (mock source, mock downloader), unlike
   the untestable bash.

## Slice plan (each retires shell script surface)

1. **`ArtifactSource` + `ArtifactSpec`** — the trait; make the existing `AVATAR_CATALOG`
   the first impl (it already has url/source_kind/license from #1871). Retire nothing
   yet; establish the seam.
2. **`ModelSource`** — second impl over `model_registry::catalog` (gguf_hint = url,
   derived path already works post-unhardcode). Two impls = the outlier-validation the
   trait needs ([[mine-past-work-for-patterns-clever-vs-typical]]).
3. **`Downloader`** — the one resumable/verified fetch; port the avatar + model curl
   logic into it; the two download-*.sh become one-line shims.
4. **`Prerequisite` manifest** — cmake/rust/node/ffmpeg/llama-server as data; `check()`
   + fail-loud `install_hint()`; retire `preflight.sh` / `setup-rust.sh` check logic.
5. **`Provisioner` + `uu provision`** — the orchestrator + the single command; the core
   self-provisions its `ProvisionPlan` on launch. `install.sh` collapses to: install
   the ONE prerequisite (rust) → build → `uu provision` → launch.
6. **Reconcile + delete** — one `install.sh`; the download/setup scripts become shims
   or are deleted. The "single command launch" is now real and testable.

## Current seams (files)

- Avatar source (exists): `live/avatar/catalog.rs` (`AVATAR_CATALOG`, #1871),
  `tools/scripts/avatar-catalog.json` (the projection).
- Model source: `model_registry/catalog.rs` + `model_registry/artifacts.rs`
  (`resolve_gguf` already derives — the resolver half of `ArtifactSource`).
- Demand: the persona/tier → model resolution + the inference-lane planner
  (`inference/serving_plan.rs`) supply the `ProvisionPlan`.
- The scripts to retire: `install.sh` ×2, `download-models.sh`, `download-avatar-models.sh`,
  `setup-rust.sh`, `preflight.sh` (`start-server.sh`/`install-service.sh` stay as thin
  launch/upstart shims over the built core).
