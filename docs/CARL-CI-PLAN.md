# Carl-Grade CI: closing the broken-merge gap

**Status:** plan / in-progress on `fix/install-carl-mac-windows`
**Owner:** anvil (mac), green-022a (windows), bigmama-wsl (linux/cuda)
**Driver:** anvil

## The problem we're solving

#950 merged with the install path on Mac doing a hidden 5-15min Rust source
build despite the README claiming "Docker-first: pulls pre-built images, no
compilation needed." The CI gates that exist today (verify-architectures,
verify-after-rebuild, validate, install-and-run-gate) caught:

- Multi-arch presence at `:pr-N` ✅
- Per-arch revision label matches HEAD SHA ✅
- TS/Rust compile clean ✅
- docker-compose-up + widget-server health responds ✅

What they did NOT catch:

- **Carl's actual install command** (`curl install.sh | bash`) was never
  exercised by CI.
- **README claim** (no compilation needed) vs **install.sh behavior**
  (5-15min Rust build on Mac) was never reconciled.
- **First chat message** the user would send was never validated to produce
  a clean response (no `<tool_use>` XML, no vision hallucination).
- **Browser-loaded UI** was never verified to actually render and accept
  user input through the same path Carl would use.

So #950 went green on its CI gates but Carl's install experience is
materially different from the README's promise. That's the gap this work
closes.

## Design principles

1. **Test the user's path, not a CI-only path.** The same `install.sh` that
   Carl invokes from `curl ... | bash` runs in CI. No CI-only smoke
   substitutes.

2. **Test the user's first action, not just service health.** After install
   succeeds, CI sends a chat message + an image, and asserts the response
   reads like a non-broken product (no XML leak, no hallucination markers,
   real Vision description).

3. **Cross-platform from day one.** amd64-linux is mandatory; arm64-mac is
   high-priority via self-hosted runner OR developer-pre-push gate; Windows
   (via WSL2 or PowerShell) is third tier but not optional.

4. **Conservative-by-default required-checks.** New gates added as REQUIRED
   in the PrimaryBranches ruleset only after they demonstrate <2% false-fail
   rate over 1 week. False positives erode trust faster than they protect.

5. **Same script for CI and humans.** Per Joel 2026-04-23: "make your own
   testing easy." Every gate is a one-line shell invocation any of us can
   run locally in 30 seconds.

## What lands in THIS PR

### A. Carl-install validation in CI (the headline)

A new CI job `carl-install-and-chat-smoke` that:

1. On a fresh ubuntu-latest GHA runner (amd64), does:
   ```
   CONTINUUM_DIR=/tmp/carl-probe \
   bash <(curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/$GITHUB_SHA/install.sh)
   ```
   The actual install path Carl runs.

2. Times the install (target: <15 min for the Carl-mode docker-only path).

3. After install completes, hits `http://localhost:9003/health` (existing
   health check, kept) PLUS a new `chat-smoke` script:
   - POSTs a chat message ("hello, who are you?") via the REST API
   - Waits up to 60s for a response
   - Asserts response: no `<tool_use>` XML, no `<persona-name>:` prefix,
     >100 chars, doesn't claim it cannot do something it actually can

4. POSTs a chat message with an image attachment (test fixture
   `test-data/images/image-2.jpg` — small, public CC0):
   - Asserts Vision AI's response describes the actual image content
   - Asserts non-vision personas EITHER skip the response OR honestly say
     they cannot see images (no hallucinated content)

5. Tears down. Captures docker logs on failure to GHA artifacts so we can
   diagnose without re-running.

**Required check:** `carl-install-and-chat-smoke` becomes required for
canary→main promotion (after 1 week of <2% false-fail rate to confirm
stability). For PR→canary promotion, it's required from day one — canary
is where we discover regressions, that's its job.

### B. Mac-mode install rationalization

**Update 2026-04-25 (anvil, after reading install.sh:118-123):** B.1 is
not a choice we have. Apple's hypervisor blocks GPU passthrough to
containers (confirmed by Docker Feb 2026, comment in install.sh). Mac
NEEDS to run continuum-core natively for Metal acceleration. The 5-15min
Rust build is architectural, not a bug. Going with B.2.

**B.2 (current plan):** README updated to admit the hybrid split:
- Linux: docker-first, no compilation (matches the existing README claim)
- Mac: docker for support services + native continuum-core for Metal
  (~10min first build, incremental after; happens automatically as part
  of `curl install.sh | bash` — no separate command, no env flag)

Implementation:
- README's headline install section gets a small per-platform table or
  inline note explaining the wall-clock difference.
- install.sh prints an upfront banner on Mac estimating build time
  (so Carl knows to expect ~10min, not ~3min).
- `--quiet` mode keeps existing behavior; just clearer messaging.

(Considered B.3: ship TWO install commands — install-mac.sh vs install.sh.
Rejected: more docs surface, more drift risk, fragments the support story.
One entry point with honest messaging beats two entry points with shorter
average time.)

### C. Browser smoke test (puppeteer)

Within the same CI job, after install + chat-smoke pass:

1. Launch headless Chrome via puppeteer
2. Navigate to `http://localhost:9003/`
3. Assert page loads (no chrome-error://)
4. Type "hello" into the chat input
5. Assert response renders within 30s
6. Capture screenshot for the GHA artifact (so we have visual evidence)

Catches the chrome-error trap class of bug — when widget-server isn't ready
fast enough, browser stays in a recoverable state.

### D. install.sh idempotence and friendly retry

When install.sh is interrupted partway (Carl Ctrl+C's, network drops),
re-running should resume from where it left off, not retry from scratch.
Specifically:

- Skip `git clone` if repo already at $CONTINUUM_DIR with correct origin
- Skip `docker compose pull` if all images present locally with current tags
- Skip prereq install steps that already report installed
- ONLY repeat the failed step + everything after it

Most of this is already in install.sh's check-then-install pattern; verify
end-to-end and document the resume behavior in the README.

### E. Browser pre-open delay

install.sh currently opens the browser after compose-up returns. compose-up
returns when containers START, not when widget-server is HEALTHY. Result:
chrome-error trap when browser hits localhost:9003 0.5 sec before the
server is listening.

Fix: install.sh polls widget-server `/health` with a 60s timeout BEFORE
running `open http://localhost:9003/`. If health doesn't come up, print a
human-readable timeout message + log dump command instead of opening the
browser to an error.

### F. Friendlier first-fail messaging

When install.sh fails (any phase), the error output should:
- Name the phase (`Phase 4/8: Python ML environment`)
- Show the actual failing command + its stderr
- Print 1-line guidance for that specific failure ("If pip install timed
  out, retry: `python -m pip install --retries 5 ...`")
- Capture full log to a clipboardable path (`/tmp/continuum-install-*.log`)

Carl shouldn't have to read the script source to understand what broke.

## What does NOT land in this PR (deferred to follow-ups)

- **Self-hosted GPU runner** (bigmama's box as a GHA runner) — bigger
  infra lift, do once Carl-install-and-chat-smoke is stable on amd64.
- **Persona-airc bridge** (#967) — separate value stream.
- **(d) tool_use XML parser fix** (#76) — the `chat-smoke` step in this PR
  ASSERTS clean output, so #76 is now a hard prerequisite for the smoke
  to pass. Decide: fix #76 first then ship this PR's smoke as required, or
  ship the smoke as advisory until #76 lands.
- **Recipe substrate** (#71/#73) and **Phase C paging** — independent
  workstreams, queued.

## Rollout

1. **This PR adds the smoke + the Mac-mode rationalization** to canary.
2. CI runs the new smoke as ADVISORY (not blocking) for 1 week to gather
   false-positive rate data.
3. After 1 week of <2% false-fail, flip to REQUIRED via the PrimaryBranches
   ruleset (gh api PUT).
4. Canary→main promotion is gated on the smoke passing.
5. New install regressions become impossible to merge without explicit
   `--no-verify` (which the team's standing rule forbids per Joel).

## Per-platform validation

`scripts/main-promotion-gate.sh` is the single entry point for canary→main
release receipts. Canary PRs should keep using focused Rust/TS proof; promotion
to `main` requires receipts from the machines that can actually prove each
hardware path.

| Platform | Validator | Notes |
|---|---|---|
| linux/amd64 | GHA runner (`ubuntu-latest`) | Always-on. Carl's dominant platform per HF data. |
| linux/amd64 + CUDA | bigmama-wsl box, eventually self-hosted runner | Real Nvidia Carl path; run `CONTINUUM_RELEASE_PUSH_IMAGES=1 CONTINUUM_GATE_RUN_HEARTBEAT=1 scripts/main-promotion-gate.sh`. |
| linux/amd64 + Vulkan | Linux AMD/Intel GPU host | Real Vulkan Carl path; run `CONTINUUM_RELEASE_PUSH_IMAGES=1 CONTINUUM_GATE_RUN_HEARTBEAT=1 scripts/main-promotion-gate.sh`. |
| darwin/arm64 + Metal | anvil mac (manual probe), eventually puppeteer-on-mac in CI | Dev's dominant platform; run `scripts/main-promotion-gate.sh` for local receipt and add `CONTINUUM_RELEASE_PUSH_IMAGES=1` when publishing arm64 slices. |
| windows + WSL2 + CUDA | green-022a (manual probe), bigmama-wsl secondary | Carl's secondary platform; WSL2 uses the same linux/amd64 CUDA receipt script. |
| windows native (powershell) | green-022a (manual probe via install.ps1) | New platform — rely on green's dogfood |

Each push to canary should have focused local evidence. Canary→main promotion
must collect the Mac/Metal, linux/amd64 CUDA, and linux/amd64 Vulkan receipts
or link a typed issue explaining the missing host. Missing hardware is not a
reason to weaken the runtime into CPU fallback.

## Success criteria

- [ ] Carl-install-and-chat-smoke runs on every PR; passes for unchanged-
      install diffs in <15 min.
- [ ] README's "Docker-first: no compilation needed" claim is true on all
      platforms (Carl mode default).
- [ ] Browser smoke catches the chrome-error trap class.
- [ ] After 1 week, smoke is REQUIRED in the PrimaryBranches ruleset.
- [ ] No future PR can land that breaks Carl's install without explicit
      bypass (which the team's discipline forbids).

## Coordination

- **anvil:** drives the plan, implements A (Carl-install smoke), B
  (Mac-mode), E (browser pre-open delay), F (friendlier failures).
- **green-022a:** drives the install.ps1 / Windows-native parity with the
  shared logic in `src/scripts/lib/install-common.sh`. Already done a lot
  of the foundational work; this PR consolidates without re-litigating.
- **bigmama-wsl:** Linux/CUDA Carl probe (manual, for ground truth before
  self-hosted runner lands), reviews + maintains the Linux side of
  install-common.sh. Eventually owns the self-hosted GPU runner.
- **joel-mac-dm:** out of scope unless airc-side identity work surfaces a
  conflict; airc PR #70 already shipped what we need for #967 anyway.
- **joel:** approves the README-vs-behavior reconciliation choice (B.1 vs
  B.2) and the timing of "advisory → required" transition for the smoke.
