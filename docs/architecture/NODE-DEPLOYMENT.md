# A node's deployment is one typed record

Joel, 2026-10-27: "Thought we worked out how to use non hacked paths and use a formal structure
for state ubiquitously." A mind's context has one codec (`continuum-context`). A node's deployment
had no such structure until now. It was five facts with five owners, each right locally, and
nothing checked the combination.

## What went wrong (the M5, 2026-10-27)

- 14:05Z: `continuum install --core` run from a worktree built the tracked main checkout
  (8e38ada8d). That checkout pinned fork e7e1077b3, and the install promoted it into `engine-a`.
- Canary 29d509098 (15:11Z) and #5526 280256918 (16:25Z) both pin 741b0175b, which was in
  `engine-c`. Both deploys used `reboot --prebuilt` and printed "deploy verified" about the core
  alone. For 2.5 h the node ran an engine neither source pinned.
- That engine saves pages with only their head checkpoint. A restore that matched 151,069 of
  154,040 tokens resumed at 11,159 and re-prefilled ~140k per resident. That was most of the
  day's restart cost.

## The record

| Fact | Owner | Known how |
|---|---|---|
| core build | `continuum-core/build.rs` | `CONTINUUM_BUILD_GIT_SHA`, compiled in |
| engine pin | `vendor/LLAMA_CPP_PIN` | compiled in (`llama_server::engine_pin::PINNED_ENGINE`); `source-hygiene::vendor_pin` keeps it equal to the gitlink |
| engine slot | `engine_slots` | the `current` file; each slot's `.llama-server.stamp` (`<commit>:<backend>[:<arch>]`) and `.verified` |
| backend | the installer | the slot stamp's second field |
| saved pages | `saved_ledger` | `KvPageContract` fingerprint (model, adapters, cache type, flash-attn); the engine build is excluded on purpose, and llama.cpp's own state format version decides compatibility |

The rule: **the running core holds its node to its own record.** Nothing outside the core
decides which engine a core runs.

## What holds it (this change)

- `engine_pin::hold_current_engine_to_pin` runs before every launch (`prepare.rs`) and before the
  reconcile compares a running lane with the installed engine (`reconcile.rs`). An adopted lane
  therefore converges too.
- When the current slot's commit is not the pin, a slot holding the pin on the same backend is
  promoted, a verified slot first. The reconcile's existing convergence (#4464) then relaunches
  the lane onto it.
- When no slot holds the pin, the node keeps serving on what it has and fires
  `serving.engine.unpinned` once. A mute resident is worse than a slow one; the deploy receipt
  below fails instead.
- Never under an operator's `LLAMA_SERVER_BIN`, and never while a deploy claim is in flight (the
  deploy owns the pointers mid-promote).
- A candidate slot must carry exactly the current slot's verified target (`<backend>[:<cuda
  archs>]`, the installer's `STAMP_WANT`); a current slot with no readable stamp promotes nothing
  (`serving.engine.untargeted`).
- `ping.engine` keeps the INSTALLED slot stamp and the SERVED engine (the live incarnation's own
  `/props` build, OS-confirmed, and the slot stamp it was launched from) apart, or names the
  operator's own engine as its own mode. `deploy-verify` (and so every `reboot`) passes only when
  the live owned engine reports the pin and runs from the installed slot; a missing report, no
  install, an unadopted lane or a stale build refuses ("ENGINE NOT VERIFIED / PINNED / ADOPTED")
  after the serving wait (`DEFAULT_SERVING_WAIT`, one cold relaunch).

## Still open, each with an owner

- **Deploy from where you stand:** `continuum deploy --here` stages the cwd checkout's core and
  engine as one record into inactive slots, verifies both, and reverts to the last verified pair
  on failure (card 3922030f, Fable). `install --core` keeps tracking the durable checkout from any
  directory (Joel, 2026-09-19).
- **The Unix stamp names the working-tree submodule HEAD, not the gitlink**
  (`install-llama-server.sh:65`); Windows already reads the gitlink (`win-engine.ps1:13`). It
  should read the gitlink on every OS (card a3e92354).
- **One process per module (the placement map):** each module's record joins this one when its
  crate runs as its own process.
