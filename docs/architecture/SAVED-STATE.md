# Saved state: one contract for everything that is saved and restored

> Joel, 2026-11-07: "make sure this is handled elegantly at the trunk not at each leaf. State saving and loading is everywhere."

Status: DESIGN (seam before code). Inventory read from origin/canary a5134e1a2 (read-only). Owners: Fable (contract, KV pages, view/volatile), Cormac (grants, ORM adapter), Codex (captures, engine stores).

# Saved state: an inventory and a proposal for one shared contract

Read-only inventory. continuum `origin/canary` a5134e1a2 (2026-11-07 10:17 -0600). airc `origin/main` 6f712f73.
The act-time fit is on the unmerged branch `origin/fable/context-has-a-price` 1fa12e9cb.
Nothing was built or run. Every claim below cites the file it was read from. Where a statement is an inference rather than something read, it says so.

Serves: persistent minds that wake where they left off (warm KV, working memory, view, measured need) after every deploy, relaunch or page-out. The goal is that each kind of state stops inventing its own save, check, rotation and silent fallback. Order-of-work lines: 3b (KV paging owned by us), 2 (epoch view reuse), and phase 5 (move a repeated pattern down to one owner).

## 0. What already exists to build on

Two shared layers already exist. Both handle bytes; neither knows anything about the meaning of what is saved.

| Layer | File | What it gives | Adopters on canary |
|---|---|---|---|
| `continuum_host::durable` | `core/continuum-host/src/durable/mod.rs` | `replace` (rename then sync the dir; retries on Windows sharing errors), `publish_new` (write once, content-addressed), `fence`, `write_replace`, `Staging`, `lock`/`lock_path` | about 20 call sites: saved_ledger, engine_slots/attempts, served_window_store, expert_placement, seed, training_demand/rate, governed_training_free, job_dir, deploy_ledger, recovery_journal, capped_appender, citizen_workspace_pool, lease_pull |
| `disk_reporters::ClassWriter` | `core/continuum-kernel/src/system_resources/disk_reporters/class_writer.rs` | each writer names a **class** (a `TrackedDir` row in `standard.rs`, which carries an eviction decision in `disk_eviction/eviction_decisions.rs`) plus a relative path. It adds confinement (`confine`), `replace`/`publish_new`/`append`/`lock`, and `owned_by(uuid)` / `owners()` / `files()` for listing everything back at boot | working_set (demand/emission), tool_executor/spill, teach/progress, gym_grader, format_exam_lesson, engram redaction, inference coordinator/lane |
| ORM `OrmStore<T>` | `core/continuum-kernel/src/orm/{store,entity}.rs` | typed rows; `BaseEntity.version` (an optimistic-concurrency counter, **not** a schema version); new columns evolve additively on first write (`derive_test.rs` EvolvingV1->V2); decode errors are typed | about 30 collections: dreaming training_* (6), mind credit/obligation/wakefulness (18), genome exam/recall receipts, engram, experience recipe_runs, hands port_leases, kernel identities/grant_epoch_watermarks/disk_cache_roots |
| ratchet `writes_outside_a_class` | `core/source-hygiene/src/budget_discipline/writes.rs` | flags `fs::write(` / `File::create(` / `OpenOptions::new()` outside ORM, disk_reporters, durable and read_pin | rough count from my grep: about 560 sites, inline test mods included. This is my count, not the ratchet's own number |

Already built: atomic placement, classing, confinement, ORM rows.
**Missing:** a shared **meaning** layer that covers identity, format version, validation against the live world, a typed load verdict, a save receipt, retention by policy, and one probe vocabulary. Every owner below rebuilds that layer by hand, and they mostly differ in how silently they fail.

## 1. Inventory

Column key. **Fmt/ver**: format and whether there is a version field. **Load validates**: what is checked, and what happens on a mismatch. **Confirm**: what proves a save happened. **Retention**: is it tracked in `standard_tracked_dirs` (T), does it have an eviction decision (E), and how is it bounded. **Probe**: what fires on failure.

### 1a. Engine and serving (continuum-engine)

| # | Owner (file) | State | Storage | Fmt / ver | Load validates; on mismatch | Confirm | Retention | Probe | Known defects |
|---|---|---|---|---|---|---|---|---|---|
| E1 | `inference/slots/{saved_ledger,page_key,citizen_slots,kv_slot_pool,page_io,relaunch_discard}.rs`, `llama_server/pages.rs` | **KV pages**: one per mind `a-<persona>.bin`, one per fork `a-<p>-fork-<room>.bin`, sidecars `.ckpt` and `.lora` | `~/.continuum/cache/kv-pages/<model>/` (`kv_pages_root` **hand-builds the path from `dirs::home_dir`, not from `continuum_home`**) | binary engine state (the engine checks `LLAMA_STATE_SEQ_MAGIC/VERSION` and the fork's CKPT magic) | the core does not read the page itself; eligibility comes from the ledger (E2). The engine refuses a page that does not fit, and turn_admission/page_receipt classifies that as `Terminal::Rejected` with an untyped message string | `n_saved` receipt that names the slot and file (`page_receipt::terminal_receipt`), then the ledger | T `kv-pages`, E "#155 broker pool... generation sweep". In practice: `trim_page_store_after_save` runs **only after a save, only on the live PAGE_DIR**, oldest first, above **`KV_PAGE_STORE_MAX_BYTES` = 128 GiB (a constant with no measurement cited)** and never on a page younger than 24 h | `inference.kv_page.{store_trimmed,store_unreadable,settled,stop_checkpoint*,discarded_by_relaunch,uncertain}` | **Fork pages leak.** Forks are kept out of the ledger ("die with the core"), but their files stay on disk and only the 128 GiB budget can remove them. 1.5 GB of forks never reaches that budget, so the 6 stale fork pages are never drained. The dirs of other models are never trimmed. **A restore that diverged has no typed signal**: the fingerprint deliberately leaves out window and slot count and relies on the engine's own refusal, and a restore that is accepted but only partly reused shows up as a lower cached count, never as `Diverged{at}` |
| E2 | `slots/saved_ledger.rs` | **confirmed-save ledger**: which minds have confirmed pages, the engine incarnation (pid, start), mind residents per slot | `<page dir>/saved-ledger.json` | JSON, `format: u32` (=2), `fingerprint` (sha256 of model/adapters/draft revisions, cache type, flash_attn; `KvPageContract::fingerprint`) | the best validation in the tree. Missing file: empty. Unreadable, malformed, or another format/fingerprint: **probe `ledger_unread` + empty set (the pages are still on disk, unlisted)**. Format 1: dated against an identity floor. Residents are kept only for the same incarnation; adoption gives a typed `AdoptRefusal{NoLedger,Unreadable,Unstamped,AnotherProcess}` | it is itself the confirmation (a key enters only after the engine confirms the save, and leaves before an overwrite). Write: tmp + `durable::replace`, generation-ordered | inside kv-pages | `ledger_unread`, `ledger_keys_dropped`, `ledger_migrated`, `ledger_folded`, `fold_unmoved` | a mismatch is a probe string (`why="written under another page format"`), not a returned variant. Callers cannot branch on it |
| E3 | `inference/served_window_store.rs` | last served per-slot window (geometry) | `<home>/state/served-window.json` | JSON, no version | probes `serving.window.store_{unreadable,corrupt}`, then none | `serving.window.stored` probe; `durable::Staging` | **`state/` is not a tracked class** | yes | is the `state/` dir governed at all? |
| E4 | `inference/{decode_knee,prefill_rate,serving_rates,lane_footprint}.rs` | per-model measured curves and rates (EMA) | `state/{decode-knee,prefill-rate,serving-rates,lane-footprint}.json` | JSON, no version (serde defaults) | `from_slice(..).unwrap_or_default()`: **corruption is silently forgotten** ("never trusted"), no probe | none (`let _ = rename(tmp, path)`: **a failed rename is ignored**); **plain `fs::rename`, not `durable::replace`** | not tracked; bounded by model count; **never forgets old models or old engine builds** | none on load or save failure | four copies of the same load/save code in one crate |
| E5 | `inference/engine_residency.rs` | which lanes and engines hold memory | `state/engine-residency.json` | JSON | probe `inference.engine_residency.unread` | plain rename | not tracked | yes | |
| E6 | `inference/lane_registry.rs` | lane records (pid, port, model, placement) | `~/.continuum/run/lanes/*.json` (hand-built from home_dir) | JSON | `from_str(..).ok()`: **a bad record is silently skipped** | tmp + rename | not tracked (`run/`) | none on skip | |
| E7 | `serving_pin_store.rs` | operator pin of the model this host serves (intent) | `state/serving-pin.json` | JSON | probes `serving.pin.store_{unreadable,corrupt}`, then none | `serving.pin.stored` probe; plain rename | not tracked | yes | |
| E8 | `inference/engine_slots/attempts.rs` | per-slot admission attempt record | `<slot>/.engine-attempt.json` | JSON `schema: u32` = 1, `stamp` | `schema != 1 or empty stamp` gives an Err string | `durable::replace` | inside the slot dir | – | the only engine store besides the ledger with a schema check |
| E9 | `inference/weights_stamp.rs`, `llama_server/placement_stamp.rs` | which weights file the lane opened (path, size, mtime, file id); lane placement | on the lane record (E6) | JSON camelCase | typed "why the bytes cannot be vouched for" | – | – | – | validates a world fact (file identity) at load time. A pattern worth keeping |
| E10 | `llama_server/genome.rs` `ENGINE_PROBES` | per engine binary: does it accept `--train-dir` | **memory only** (`Mutex<BTreeMap<String,ProbeOutcome>>`) | – | keyed by the **binary path string**, not its content identity | – | lost on restart, so the probe runs again | – | 11-07: an unasked flag was read as "no", which caused a cold relaunch (doc comment). The key is a path, so a binary replaced at the same path keeps the old answer (inference) |
| E11 | `llama_server/gene_pages.rs` | gene residency, device to host RAM (fork #72) | in the engine process | – | asks the engine (`GET /lora-adapters`) | page_in/page_out reply | – | – | the live process is the authority, which is correct; nothing saved |
| E12 | `capacity/window_grant{,/lease,/lease/fair}.rs` | held grants per mind, lease revisions | **memory only**; lease publishes "engine-terminal progress survives finish/restart **between publisher ticks**" | – | held state rebuilt from need (C6) on restart | – | – | – | grants come back cold after a restart; the input that persists is need (C6) |
| E13 | `ai/embedding/embedding_cache_max.rs` | content-addressed embedding cache snapshot | under the cache (flushed periodically) | JSON | probe `embedding.cache.loaded` | tmp + rename | (inside core-artifacts? not verified) | `embedding.cache.flushed` | |
| E14 | `inference/child_log.rs` | llama-server logs | `~/.continuum/logs/llama-server-<port>.log` (+ rotated) | text | – | – | T `logs` | `serving.llama.log_sink_*` | |

### 1b. Mind and cognition (continuum-cognition, continuum-context, continuum-capture, continuum-mind)

| # | Owner | State | Storage | Fmt / ver | Load validates; on mismatch | Confirm | Retention | Probe | Known defects |
|---|---|---|---|---|---|---|---|---|---|
| C1 | `cognition/persona_workspace/{own_speech_persisted,root_acting_workspace,default_workspace_capacity,checkpoint_adoption}.rs` + `working_memory.rs` `VolatileSnapshot` | **the volatile mind**: WM entries, actions, thoughts, handoff, acting_card, open_views, receipt_children, own-speech rings by room, **`context_view` (continuum-context epoch view incl. `Motion` + `Rereads` births)** | `~/.continuum/personas/<id>/volatile.json` + `.volatile.lock` (**path hand-built via `paths::home_dir().join(".continuum/personas")`, not `continuum_home` and not ClassWriter**, while working_set uses the `personas` class) | JSON. **No format version**: about 15 `#[serde(default)]` fields carry history; `build_sha` is recorded; `OwnSpeechPersisted` untagged Legacy/ByRoom | strict decode, and an Err fails registration ("only an absent checkpoint permits a fresh resident"). **But `context_view` uses `deserialize_with = derived::or_none`: an undecodable view is silently dropped** (her next act starts a new epoch). Legacy own-speech is dropped with a probe. `build_sha` is shown to her as a fact, never checked | lifecycle receipt (periodic and shutdown `SaveReason`). **Own code:** File::create + sync_all + rename + `sync_parent`. It duplicates `durable::replace` | T `personas`, E "#155/#437 never blind-LRU'd, ghost dirs only". **Owner not built** | `persona.volatile.restored`, `...own_speech_legacy_dropped` | the one save carries 15 sub-states with 15 separate migration paths |
| C2 | (branch) `cognition/working_memory/act_times.rs` + `continuum-context/src/price.rs` `ActTimeFit` | per (provider, model) running least-squares act-time fit | inside C1 `VolatileSnapshot.act_times` | serde default, empty when absent | keyed by **provider-id string + requested model name**: no engine build, quant or node in the key | as C1 | as C1 | – | **never forgets**: running sums `acts, sxx, sxy...` with no decay or window. A model whose name is unchanged after an engine or quant upgrade keeps feeding the old fit (stale data used silently) |
| C3 | `continuum-context/src/{motion,rereads}.rs` | `Motion` (per-source tracks, last_order, last_epoch_acts) and `Rereads` (curves, acts, **births** map) | inside C1 `context_view` | serde with `#[serde(default)]` per added field; `fingerprint` uses `DefaultHasher` ("fixed-keyed... a toolchain change costs one mismatched comparison") | as C1 (`or_none`: the whole view is dropped on any decode failure) | as C1 | `births` drained at each epoch start to the items the sources still offer | – | a hash that depends on the toolchain is part of the persisted identity |
| C4 | `continuum-capture/src/{prompt_capture,prompt_capture_store,capture_owner}.rs` | **prompt captures** (request + terminal per call), two segments per persona | `<continuum_home>/fixtures/prompt-captures/<persona>{,.prev}.{jsonl,pidx}` | JSONL + fixed-width index, `schema_version: 4`, segment `generation` uuid | `Segment::open` checks schema 4, generation pair, entry bound; an integrity issue leads to **rotate** (the current segment goes to .prev, the older one is deleted). Reader `decode` gives an Err string | the index slot is written after the payload (`write_slot` + flush); there is no fsync | T `fixtures` (eviction "re-creatable"?), self-bounded to 2 x 64 MiB x 1024 entries per persona | **write failure: `tracing::warn!` only** (`"prompt lifecycle capture incomplete"`), not a probe; open failure at spawn: `tracing::warn!` "runs without verbatim capture" | **Likely cause of "captures stopped for two minds" (inference from reading, not reproduced):** in `Store::append`, a failed index write runs `self.data.take(); self.index.take()`, and every later append then fails at `"capture writer unavailable"` *before* the rotate check, so the sink is **dead until the process restarts**. Each failure is one `warn` line with no probe, no counter and no reopen. The capture owner lease shares that dead `Store` across forks |
| C5 | `mind/src/persona/mind_state.rs` | cursors per stream, continuation, attention dial | `<peer dir>/mind-state.json` | JSON `version: u32` = 1 | `version <= 1` is accepted; newer: probe `mind.state.newer_version`, then **fresh mind**; undecodable: probe, then fresh | tmp + rename | inside personas | yes | the only mind store with an explicit version and a typed outcome per case, but every case collapses to "fresh" |
| C6 | `cognition/working_set.rs` (+ `working_set/need.rs`) | measured window demand per persona (peak, last, floor) + emission or need sample rings | `personas/<id>/{working-set,emission}.json` **via `ClassWriter`** | JSON, no version (`Observed` untagged: "an older slot reads exact") | decode error: `tracing::warn!`, re-measure; boot `rehydrate_all` over `class.owners()` | ClassWriter `replace` | T/E personas | `working_set.rehydrated{,_all}` (tracing info) | the model adopter of ClassWriter |
| C7 | `mind/src/persona/{held_claims,roster_hold,resting_seat,grid_roster_memory,model_override,seed}.rs` | claims she holds, operator roster hold (+ `.lapsed`), resting seats, grid roster heard, model override, seed identity | `held-claims.json` (`.partial` staging), `<home>/roster-hold.json`, `state/resting-seats.json`, `state/grid-roster-heard.json`, `personas/<name>/seed.json` | JSON; **seed is `#[serde(tag = "version")]`**, the others have none | held_claims: `unwrap_or_default` (**silently empty**); roster_hold: `.ok()` (**silent**); resting/grid roster: probe, then empty; seed: typed `PersonaSeedError::Malformed` | seed uses `durable::replace`, the others use their own rename | personas tracked; `state/`, the home root not | mixed | six stores with six policies |
| C8 | `mind/src/persona/recorder.rs` | per-turn cognition recordings | `fixtures/persona-respond/*-rust.json` | `schemaVersion: 1` | – (write-only diagnostics) | – | T fixtures | – | |
| C9 | `cognition/tool_executor/spill.rs` | every tool result, content-addressed | `tool-output/<persona>/...` via ClassWriter `publish_new` + `fence_existing` | content hash is the identity | reader `fence`s an existing file | `publish_new` + fence | T/E `tool-output-spill` pool, oldest first, protects handles referenced by C1 (`persisted_result_handles`) | – | **the best outlier for "immutable, content-addressed"** |

### 1c. Genome, dreaming, academy

| # | Owner | State | Storage | Fmt / ver | Load validates; on mismatch | Confirm | Retention | Notes |
|---|---|---|---|---|---|---|---|---|
| G1 | `dreaming/modules/training_trigger/durable/*` | submissions, dispatches, retirements, retry decisions, policy corrections | **ORM** collections `training_trigger_*` | BaseEntity + indexed columns | typed ORM decode | the ORM write; intent before effect ("an interrupted provider call is never repeated on absence of proof") | DB | the best outlier for "typed row with an intent and receipt protocol" |
| G2 | `dreaming/commands/genome/attempt_ledger.rs` | training attempt evidence, written **before** the trainer call | ORM `training_attempts` | | typed | yes, by construction | DB | |
| G3 | `genome/fine_tuning/job_board.rs` | job board ledger | `genome/jobs-ledger.jsonl` (append) | JSONL | replay | – | inside `genome-models`? (not verified) | an append-only ledger next to ORM rows that hold related facts |
| G4 | `genome/fine_tuning/{job_dir,native_jobs}.rs` | per-job dir, `request.json` | `<genome root>/<persona>/<trait>/<job>/` | JSON | typed readers | `durable::publish_new` | genome-models | |
| G5 | `genome/fine_tuning/{training_demand,training_rate,training_hold_store}.rs` | parked demand per persona; secs per char per base; pause holds with a TTL | `genome/training-demand/<p>.json`, `genome/engine-rates.json`, `state/training-holds.json` | JSON, no version | demand/rate: `unwrap_or_default` (**silent**); holds: Err string | demand/rate use `durable::replace`; holds use a plain rename | genome tracked; `state/` not | TTL is the only age-based retention in the inventory besides KV |
| G6 | `genome/adapter_manifest.rs`, `gene_trial.rs` | loadable genes per base; trial rows | `forge/gguf-lora/manifest.json`; `<dir>/<persona>.jsonl` | JSON / JSONL | parse error gives an Err string | plain rename / append | T forge | `genome.trial.ledger_unwritten` probe |
| G7 | `genome/teach/progress.rs` | run ledgers | `progress/teach/<run>.json` via ClassWriter | JSON | – | ClassWriter | T progress, E "owner when built" | |
| A1 | `academy/.../{coursework,gym,benchmark_*,swe_*}.rs` | course and benchmark state | various | JSON | – | own rename | benchmarks, eval-roots | removal target (plan line 0) |
| S1 | `continuum-sentinel/src/checkpoint.rs` | pipeline resume checkpoints | `~/.continuum/sentinel/checkpoints/<handle>.json` | JSON | `from_str` error | tmp + rename | **not tracked** | |

### 1d. Lifecycle, deploy, kernel, grid, airc

| # | Owner | State | Storage | Fmt / ver | Load / mismatch | Confirm | Retention |
|---|---|---|---|---|---|---|---|
| L1 | `core/bin/continuum/deploy_ledger.rs` | deploy-consume attempts, request tip | `state/deploy-request.json`, `state/deploy-consume-attempts.json` | JSON (`Value`) | untyped Value | `durable::replace` | not tracked |
| L2 | `cli-lifecycle/deploy_claim.rs` | swap-in-flight claim (lock semantics) | `run/deploy.claim{,.waiting}` | | | | not tracked |
| L3 | `core/modules/deploy_{tracker,actuator}.rs` | deploy hold, request, actuation | `state/deploy-hold`, `state/deploy-actuation.json` | JSON | `.ok()?` (**silent**) | plain rename | not tracked |
| L4 | `cli-lifecycle/.../windows/recovery_journal.rs`, `launchd.rs`, `install_cli.rs` | supervisor install journal | – | | | `durable::replace` | |
| K1 | `kernel/system_resources/citizen_workspace_pool.rs` + `mind/work/card_staging.rs` | **card worktrees and clones**, dropped-workspace lists, `.base` | `citizens/<peer>/...`, `repos/<owner>/<name>` (+ `.partial-<uuid>`) | text lists | – | `durable::write_replace` | T/E citizens (pool: dormant workspaces only), repos |
| K2 | `kernel/routing/capped_appender.rs` | size-bounded logs | logs | text | resume from the live file | `durable::replace` on roll | T logs |
| K3 | `kernel/system_resources/harness_archive.rs` | one-time archive receipt | `archive/benchmark-harness/archived.json` | JSON | | | T/E |
| X1 | `grid/access_decision.rs` | access policy | `state/access-policy.json` | JSON | probe `access.policy.unreadable`, then a default | | not tracked |
| X2 | `expert-pager-policy/plan_file.rs` | MoE residency plan for the fork | `moe-plan.json` | `PLAN_FILE_VERSION` = 1 | | atomic write | moe-probe |
| X3 | `experience/ledger.rs` | card evidence ledger | **airc wall post body** (`CardLedger` JSON) | JSON | probe on parse failure | the post | the bus's |
| AI1 | airc `crates/airc-lib/src/work_board_cache.rs` | **work board projection cache** keyed by the cursor of the last applied transcript event | `~/.airc/work-board-cache/v<N>/<channel>/<source>` | JSON, `WORK_BOARD_CACHE_FORMAT_VERSION` = 6, **a generic trait `const VERSION: u32` over "projections resumed strictly after a cursor"** | version/channel/source mismatch, or a cursor the log disagrees with: **warn + full replay ("never trusted over the log")** | tmp + rename | T `airc-board-cache` (E deferred). Old `v<N>` dirs stay ("incompatible writers coexist"), never evicted (inference) |

## 2. The defects the brief named, located

| Symptom (2026-11-07) | Mechanism found | Where |
|---|---|---|
| captures stopped silently for two minds | `Store::append` drops `data`/`index` after a failed index write; every later append returns "writer unavailable" before the rotate path; each failure is only a `tracing::warn!`; no reopen and no probe. (Second path: `open_for_persona` failing at spawn logs a warn and runs with no capture for that life.) **This is inferred from reading the code, not reproduced.** | `continuum-capture/src/prompt_capture_store.rs` (append, around lines 300-304); `prompt_capture.rs` `append()`; `default_workspace_capacity.rs:615-624` |
| 6 stale fork KV pages, 1.5 GB, never drained | forks are left out of the ledger by design, but their files are not deleted at fork end or at boot. The only remover is `trim_page_store` above 128 GiB and older than 24 h, run after a save, in the live dir only | `slots/page_key.rs` (`as_mind`), `slots/citizen_slots.rs:259,355-...` |
| act-time fit never forgets | running sums with no decay; keyed by provider/model *names* | branch `continuum-context/src/price.rs`, `working_memory/act_times.rs` |
| restored KV diverged after a layout change, no typed signal | the fingerprint leaves out window and slot count, and leaves the engine build out on purpose (relying on engine magic). A restore that is accepted but only partly reused shows up only as a lower `cached`. A refusal is `Terminal::Rejected` with an untyped message. A ledger mismatch is a probe and an empty set, not a returned verdict | `kv_slot_pool.rs` `KvPageContract::fingerprint`, `turn_admission/page_receipt.rs`, `saved_ledger.rs::load` |

Other silent paths found along the way:
- E4's four rate stores and G5's demand/rate stores do `unwrap_or_default` on corruption, and E4 also does `let _ = rename`.
- E6 lane records and L3 deploy state are skipped silently.
- C1's `context_view` is dropped through `or_none`.
- C7 held_claims and roster_hold load as empty silently.
- `state/`, `run/` and `sentinel/` are **written but not in `standard_tracked_dirs`**: about 20 files, which breaks the 2026-07-13 rule ("a new directory gets a TrackedDir row and an eviction decision").
- Paths are hand-built from `home_dir`: kv_pages_root, volatile_path, lane_registry and sentinel bypass `continuum_home`/ClassWriter. That is plan line 7's 73 paths.

## 3. The lifecycle every owner rebuilds

Each owner re-implements some subset of these steps, each in its own way:

1. **Identity.** Whose state, and keyed by what: persona uuid, (persona, room), model id, engine binary, channel+source.
2. **Validity contract** (the facts the saved bytes depend on): the KV fingerprint (model and adapter revisions, cache type, flash attention), the engine incarnation (pid, start), build_sha, toolchain hash (Motion), the transcript cursor (airc), file identity (weights_stamp).
3. **Format version**, present in only 7 of about 45 stores (ledger `format`, attempts `schema`, capture `schema_version`, mind_state `version`, seed tag, plan file, airc cache). The rest rely on `#[serde(default)]` accretion.
4. **Save**: stage, write, place atomically. About 20 call sites use `durable`; about 15 still hand-roll `rename`, some with `let _ =`.
5. **Confirm**: a receipt that the save completed and is eligible. Only KV (engine `n_saved` then ledger), ORM intent rows, spill `fence` and capture index-after-payload do this. Everything else treats "rename returned" as "saved", or ignores even that.
6. **Load and validate**, producing in effect one of `Absent | Matched | Diverged{what, at} | Stale{age or contract} | Corrupt{why} | Newer{found, known}`. Today these collapse into: default (silent), a probe plus default, an Err string, or (rarely) a typed enum (`AdoptRefusal`, `PersonaSeedError`, `Terminal`).
7. **Migrate or fold**: the ledger fold of per-activity pages into per-mind pages, own-speech Legacy, legacy page dirs, serde defaults. Each is one-off code that is never deleted.
8. **Retention**: ages (24 h KV, hold TTL), budgets (128 GiB KV, 2 x 64 MiB captures, 16-sample rings), generations (capture `.prev`), "owner not built" (personas, progress), or nothing (`state/`, fork pages, the act-time fit, airc `v<N>`).
9. **Probe**: per-owner class strings (`inference.kv_page.*`, `serving.window.store_*`, `mind.state.*`, `persona.volatile.*`), plain `tracing::warn!` (capture, working_set), or nothing.
10. **Custody at stop**: page_io's `SettleBeforeStop` (KV), the C1 checkpoint gate and lock, the deploy claim. Only KV holds a stop open until its saves confirm.

Shared code today is limited to bytes: `durable` (placement, locks), `ClassWriter` (class root, confinement, owner listing), ORM (rows). Steps 1-3 and 5-9 have **no** shared owner.

## 4. The two outliers that test the contract

- **Outlier A: KV pages in the engine (E1+E2).** Binary, hundreds of MB to GB per item, written **by another process** (llama-server) and confirmed by its HTTP receipt. Validity rests on a world contract (model and adapter revisions, cache type, FA, engine incarnation). Has sidecars, a stop-time settle, budget and age retention, and the engine itself enforces part of the validation. Divergence is *partial*: a prefix of the tokens is reused.
- **Outlier B: a small typed snapshot or row.** Either `MindState` (C5: versioned JSON in the persona class) or an ORM row (G1/G2: typed, indexed, intent before effect, with no file at all).
- **Outlier C, worth keeping as a third check:** the airc work board cache (AI1). A derived projection whose validity is a **cursor into an authoritative log**. It must never be trusted over its source, and any mismatch means recompute from the source. C1's `context_view`, E4's rates and C2's act-time fit are the same kind: derived, rebuildable, and safe to drop **loudly**.

If one contract covers A (external writer, partial divergence, budgeted bytes) and B (row, typed, intent receipt) without forcing either, the remaining stores are the middle case.

## 5. Proposed contract

### What an owner declares

```rust
/// One kind of state that outlives a process. Implemented once per kind (KV page set, volatile
/// mind, served window, act-time fit...). The trunk owns save, confirm, load, validate, retain, probe.
pub trait SavedState: Sized {
    /// Probe class root and registry name, e.g. "kv_page", "volatile_mind".
    const KIND: &'static str;
    /// Bumped on any incompatible shape change; a load of another version is `Newer`/`Older`, never a guess.
    const FORMAT: u32;
    /// Whose state: a typed key (MindKey, PageKey, ModelId, (ChannelId, Source)). Never a formatted string.
    type Key: Ord + Clone + Serialize + DeserializeOwned;
    /// The facts the bytes are valid under (fingerprint inputs, incarnation, build, cursor). Compared,
    /// field by field, at load: a mismatch names the field (`Diverged { what }`).
    type Contract: PartialEq + Serialize + DeserializeOwned + Describe;
    /// Where the bytes live and who places them (see `Medium` below).
    type Medium: Medium;
    /// What kind of state: decides the default on a non-match and the eviction story.
    const CLASS: StateClass; // Authoritative | Derived { source: &'static str } | Cache
    /// Retention, declared as data, never a constant in the owner's code.
    fn retention() -> Retention; // Generations(n) | Budget(Measured) | Age(Measured) | WhileReferenced(fn)
}

/// What a load returns. Every owner and caller matches it; no `unwrap_or_default` on state.
pub enum Loaded<T, C> {
    Absent,
    Matched(T),
    /// Same key, the contract changed: `what` names the field (model revision, cache type, cursor...).
    Diverged { saved: C, live: C, what: &'static str, at: Option<u64> },
    Stale { age: Duration, policy: Retention },
    Newer { found: u32, known: u32 },
    Corrupt { why: CorruptCause },
    /// Saved but never confirmed (KV page with no engine receipt; staged row with no completion).
    Unconfirmed,
}
```

`Medium` is the only part that differs per adapter, with each adapter carrying only its own difference:
- `ClassFile`: small serde snapshot through `ClassWriter::replace` in a registered class (C5, C6, E3-E7, G5, C7).
- `OrmRow<T>`: `OrmStore<T>`. The receipt is the intent/completion row protocol already in G1/G2.
- `ExternalBlob`: bytes another process writes (llama-server pages + sidecars). The trunk keeps the **confirmed-save ledger** generically: a key is eligible only after `confirm(key, receipt)` and leaves before any overwrite. That is E2's invariant made reusable.
- `ContentAddressed`: `publish_new` + `fence` (spill, gene artifacts).
- `DerivedFromLog`: airc-style, valid only at `cursor == source.tip_or_prefix()`, so any mismatch is recompute.

### What the owner gets for free

1. **One envelope** `{kind, format, key, contract, saved_at, build, confirmed}` around every payload, with the `ExternalBlob` ledger and ORM rows carrying the same fields. One reader can list every saved state on a node (`state/list`, `state/inspect <kind> <key>`). That is the glass box for "why did she wake cold".
2. **Typed load** (`Loaded`) with a probe for every arm under one class tree: `state.<kind>.{matched,diverged,stale,newer,corrupt,unconfirmed,absent}`, each carrying key, `what` and bytes. This ends the `tracing::warn!` and silent `unwrap_or_default` paths (capture, rate stores, held_claims, lane records, context_view).
   **Read once, probe on change** (built, `continuum-saved-state/src/memory.rs`). On the M5 on 2026-11-07 (build 839e7a08d), serving built a fresh `TrainingDemand` store for each mind on every pass of `requirements_for`. One resident's file was read and probed 1,748 times in 23 minutes, and `state.load.absent` logged 9,224 lines in an hour. For `OwnedFile` kinds the store is the only writer of its files: nothing outside the core places them, and every save and remove in the core goes through a store. So the process keeps one copy per file, shared by every handle, including a fresh `Store::at`. The first load reads the disk. Later loads reuse the decoded verdict while the live contract stays the same. A contract change is judged again from the copy, so it still loads `Diverged{what}`. An `Aged` kind is judged again on every load, because its age moves. A save or remove replaces the copy with what it placed. A failed save or remove drops the copy, so the next load reads the disk again. A verdict is probed when its class or generation changes for a key, and the probe carries `served_from_memory`, the number of loads answered without a disk read since the last probe. A file this build cannot trust (Corrupt, Newer, Older) is never copied, so an operator who repairs it while the core runs is read at the next load. Any other file placed behind a running core (a hand edit) is read at the next restart. `ExternalBlob` (KV pages) and captures are written by another process or appended outside a store, so they are never copied. They revalidate against their writer's receipt.
3. **Save with a receipt**: `save()` returns `Saved{generation}` or a typed error, and **a failed save never latches the owner dead**. The trunk re-opens on the next save and counts consecutive failures in a probe, which fixes the capture latch by construction.
4. **Retention by policy, with the eviction decision derived.** `Retention` plus `StateClass` produce the `TrackedDir` row and the `eviction_decisions` entry, so `standard_tracked_dirs` stops being a hand list and the `state/`, `run/` and `sentinel/` gaps close. `WhileReferenced` lets a **fork page** (key lifetime = fork lifetime) be dropped when its fork ends and swept at boot (no live owner). That fixes the fork leak without a budget constant. Forgetting is never a retention policy: a half-life is a constant. A derived fit persists its observer state, its Contract names what it depends on (the engine config), so a change loads `Diverged{what}` and its observer widens its variance (the controller pattern, card 8a983e31).
5. **Stop custody**: every `SavedState` with in-flight saves registers as a `SettleBeforeStop` participant (generalizing `page_io`), so no save that has started is cut silently by an exit.
6. **One migration seam**: `fn upgrade(from: u32, bytes) -> Result<Self, CorruptCause>`, recorded with a probe and deleted when the ratchet sees no file older than N. This replaces serde-default accretion, untagged Legacy enums and one-off folds.
7. **A replay and validator hook** (goal 11): the trunk can round-trip every kind in a seconds-scale test (save, load, `Matched`; perturb each contract field, `Diverged{what}`), from one generic test instead of one test per owner.

### Where it lives (placement map vs crate graph)

- MODULE-ARCHITECTURE §9.2 puts persistence in **Store** ("ORM, engrams, blobs, vector search | where the disk is"). But on canary `continuum-store` depends on `continuum-genome`, which depends on `continuum-engine`, so **the engine cannot depend on continuum-store**, and KV pages (outlier A) live in the engine.
- So the **contract and its file/blob adapters go in a new floor crate named for the pattern**, e.g. `continuum-saved-state`. It sits on `continuum-host` (durable, confine) and `continuum-base` (probe!, ids), below kernel and engine, as plan phase 5 prescribes ("a NEW shared crate named for the pattern, never a utils drawer").
- `ClassWriter` (kernel) implements `Medium` for classes.
- The ORM adapter `OrmRow<T>` lives in kernel next to `orm/`.
- `continuum-store` (the 9.2 Store service) hosts the **operator surface and registry**: `state/list|inspect|evict` commands, the generated TrackedDir/eviction rows, and the boot-time sweep of unowned keys. That keeps the Store module the owner of "where the disk is" without an upward import.
- `disk_reporters::standard_tracked_dirs` becomes the union of hand rows (cargo, hf-hub, airc) and rows registered by each `SavedState` kind. `every_cache_class_has_a_decided_eviction_story` then holds by construction for every kind.
- A ratchet `state_loads_outside_the_trunk` (any `serde_json::from_slice` of a file in a tracked class outside the trunk) joins `writes_outside_a_class`.
- airc keeps its own repo. Its `VERSION`-and-cursor trait is the prior art for `DerivedFromLog` and should converge on the same `Loaded` vocabulary (an airc card, not a continuum dependency).

### Suggested order (each step is one PR, each deletes more than it adds)

1. Floor crate: `SavedState`, `Loaded`, `ClassFile` medium, the generic round-trip validator. Migrate E3, E4 x4, E7, C5 (the small snapshots), which deletes four copies of the rate-store load/save code and the `let _ = rename` calls.
2. `ExternalBlob` + generic confirmed-save ledger; move E1/E2 onto it. `WhileReferenced` closes the fork leak; `Diverged{what}` gives the layout-change signal; the 128 GiB constant is replaced by a measured budget.
3. C1 volatile split into per-concern kinds (wm, own_speech, context_view as `Derived`, act_times as `Derived` observer state) under one envelope; delete the hand-built path and the duplicated durable code.
4. Capture as an append-segment medium, with the save receipt and no latch.
5. Register `state/`, `run/` and `sentinel/` through the kinds; add the ratchet.

## 6. Open questions for the owners

- KV (Fable/Codex): should the fingerprint carry the engine build after all, now that a typed `Diverged{engine_build}` could mean "try restore, expect refusal" instead of "drop everything"? Today the choice is binary.
- ORM (Codex): should `BaseEntity` gain the envelope's `format` (schema) next to its concurrency `version`, or should the ORM row's schema evolution stay purely additive?
- Retention numbers: every `Retention` value must be measured (budget from disk pressure, age from a resident's return gap, `serving_daemon::rotation_window_ms`). The trunk should refuse a bare constant, as `KV_PAGE_STORE_MAX_BYTES` is today.

## Review answers (Cormac, 2026-11-07, review a52c79c4)

- **No `Retention::Decay`.** Forgetting belongs to the observer (controller card 8a983e31), not to persistence; see §5 item 4.
- **ORM format** is per collection, declared in its registration and folded into the envelope at `OrmRow<T>`, never per row.
- **Grants** are `Derived` from demand: dropped loudly on restart, never restored as authoritative.

## Review answers (Codex, 2026-11-07)

- **Provenance is not validity.** An engine store records the model and the engine build/config that measured a value as provenance, kept apart from the observer's freshness and variance and from persistence age. The medium never invents rate confidence or a reset policy; compatibility comes from the kind's declared format and contract, never from rejecting every other build.
- **The medium suite covers the failures, not only the round trip:** an interrupted write (payload landed, rename did not), a concurrent writer that read a stale generation, and a restart that reopens the store. Every kind runs it.
- **A save receipt binds what was written:** the generation and a digest of the placed bytes, never a bare confirmed flag.
- **Captures (append segments):** an append receipt binds the segment generation, the entry's offset, length and digest, and the index commit. A payload written before its index leaves a detectable unindexed tail; reopen reconciles or truncates it under the existing writer lock before any retry.
- **`ExternalBlob` (KV pages):** `Matched` metadata means eligible to try, never an engine-confirmed restore or prefix reuse. `confirm` binds the operation, the engine incarnation, the slot and the page revision, and the entry is invalidated before any overwrite. `Diverged { at }` comes only from the engine's report of what it reused, never fabricated from a low cached count.
