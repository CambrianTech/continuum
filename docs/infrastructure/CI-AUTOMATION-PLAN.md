# CI Automation Plan — Build For The Multi-Agent Workflow

**Status**: Plan, 2026-05-01. Phase A actively shipping.
**Origin**: live #974 meta-blocker discovery during the M5-QA + dev-tab + M1-Carl-validator parallel session of 2026-05-01.
**Top-level GitHub issue**: see [issue link to be added once filed].

## Why this exists

We're building Continuum + airc as a coordinated multi-agent project. Today's session demonstrated the workflow: M5-dev + M5-QA + M1-Carl-validator + airc mesh coordination, with continuous PRs landing through canary. To sustain that pattern, the CI must be:

1. **Repeatable.** Any future hardware contributor (Toby, anyone) can plug in without bespoke setup.
2. **Self-aware.** The right gates fire for the right kind of change. Nobody manually triggers workflows.
3. **Image-producing automatically.** When a PR touches Docker-relevant code, CI builds the images — no "did anyone remember to push?" question.
4. **Mesh-observable.** The build farm's state is visible on airc, just like every other peer's state.

Today's blocker (#974): the existing `docker-images.yml` workflow only fires on PRs targeting `main` AND only when `core/**` or `docker/**` paths change. PRs targeting `canary` (the working integration branch) silently never produce the required-status-checks `verify-architectures` and `verify-after-rebuild` that the canary ruleset gates merges on. **Result**: every TS-only or doc-only PR is permanently un-mergeable to canary.

## The architecture this plan delivers

```
                    ┌─────────────────────────┐
                    │  GitHub PR opens / push │
                    └────────────┬────────────┘
                                 ▼
                    ┌─────────────────────────┐
                    │  detect-relevant-changes │  (always runs)
                    │  ─ TS-only      → skip   │
                    │  ─ docker_relevant → go  │
                    └────────────┬────────────┘
                                 ▼
              ┌──────────────────┴──────────────────┐
              ▼                                     ▼
   ┌──────────────────────┐            ┌──────────────────────────┐
   │  TS-only branch      │            │  Docker-relevant branch  │
   │  ─ verify-arch:PASS  │            │  ─ build-amd64           │
   │    (auto-skip note)  │            │      runs-on: BigMama    │
   │  ─ verify-after-     │            │  ─ build-arm64           │
   │    rebuild:PASS      │            │      runs-on: Mac M5     │
   │    (no rebuild ran)  │            │  ─ stitch multi-arch tag │
   └──────────────────────┘            │  ─ verify-arch (real)    │
              │                        │  ─ verify-after-rebuild  │
              │                        └────────────┬─────────────┘
              └────────────┬───────────────────────┘
                           ▼
                ┌────────────────────────┐
                │  PR mergeable to canary│
                └────────────────────────┘
```

## Phases

### Phase A — Self-aware required check (THIS PR — fix/974-conditional-docker-verify)

**What.** Modify `.github/workflows/docker-images.yml`:
- `pull_request.branches: [main, canary]` — fire on PRs to either branch
- Remove `pull_request.paths` — workflow ALWAYS fires
- Add a `detect` step using `dorny/paths-filter@v3` to compute `docker_relevant` boolean
- When `docker_relevant == false`: emit `::notice` + auto-pass the job (required check satisfied without touching ghcr)
- When `docker_relevant == true`: run the existing verification flow unchanged
- Apply the same pattern to `verify-after-rebuild`
- Job-output fallback chain (`steps.skip-pass.outputs.X || steps.gate.outputs.X`) so downstream jobs read sane values regardless of which path ran

**Why.** Unblocks the 4 PRs targeting canary (continuum#976/#977/#978/#979 + the M5-QA fixes stacked on top). Doesn't require any hardware changes. Doesn't change the existing image-verification semantics — only the gating semantics for non-relevant PRs.

**Done when**: a TS-only PR targeting canary fires the workflow + sees `verify-architectures` PASS + sees `verify-after-rebuild` PASS + becomes mergeable. Then this Phase A PR itself becomes mergeable to main (via the `[main]` filter, which still fires it for main-targeting PRs since `docker-compose.yml` is in the path) → cherry-pick to canary.

**Status as of 2026-05-01 PM**: PR opening this session.

### Phase B — Self-hosted runner registration

**What.** Register continuum dev hardware as GitHub Actions self-hosted runners.

- **BigMama** (Linux + Nvidia 5090 + amd64): runner labels `[self-hosted, linux, amd64, cuda]`.
- **Mac M5** (macOS + Apple Silicon + Metal): runner labels `[self-hosted, macos, arm64, metal]`.
- Document the registration steps in `docs/infrastructure/SELF-HOSTED-RUNNERS.md` (paired with this doc) — exact `gh-runner` install + `gh repo set-default` + `./config.sh` invocation. Should be a 5-line copy-paste any future contributor (Toby, Carl, anyone) can run on their hardware to add it to the build farm.

**Why.** The existing scripts (`scripts/push-current-arch.sh`, `scripts/push-image.sh`) already do the right thing on dev hardware — they build per-arch + push to ghcr. To eliminate the "who's pushing?" question, the same hardware needs to be reachable as a CI runner so the workflow can dispatch builds automatically.

**Done when**: GHA dashboard shows BigMama + Mac M5 as online runners with the label sets above. A no-op workflow targeting `runs-on: [self-hosted, linux, amd64]` succeeds on BigMama; same for Mac arm64.

### Phase C — Automated image build on docker_relevant changes

**What.** When `detect.outputs.docker_relevant == true`, dispatch parallel build jobs:

- `build-amd64` runs on BigMama, invokes `bash scripts/push-current-arch.sh`
- `build-arm64` runs on Mac M5, invokes `bash scripts/push-current-arch.sh`
- Both push images to ghcr at `:pr-<N>` tag for the PR
- `verify-architectures` job (existing, real verification path) runs after both builds + finds the images + passes

**Why.** Eliminates manual `push-current-arch.sh` invocation. PRs that touch Rust/Docker just get their images automatically. The verify gate becomes meaningful (it's verifying images that the PR's CI itself produced).

**Done when**: a PR that touches `core/continuum-core/Cargo.toml` opens; `build-amd64` runs on BigMama + pushes the amd64 image; `build-arm64` runs on Mac + pushes the arm64 image; `verify-architectures` finds both + passes; PR mergeable.

### Phase D — Multi-arch manifest stitching

**What.** After both arch builds push, a tiny `stitch-manifest` job composes the multi-arch manifest at the `:pr-<N>` tag using `docker buildx imagetools create`. `verify-architectures` then sees both arches in one tag.

**Why.** The verify step expects a single tag with both arches. Without stitching, it would only see one arch at a time + fail the cross-arch check.

**Done when**: `docker buildx imagetools inspect ghcr.io/cambriantech/continuum-core:pr-<N>` shows both `linux/amd64` and `linux/arm64` (and `darwin/arm64` if Mac builds in the docker-darwin mode — TBD, depends on what `push-current-arch.sh` does on Mac).

### Phase E — Caching + skip-if-exists

**What.** Before invoking the heavy build, hit ghcr with a HEAD request to check if an image already exists at the SHA. If so, skip the build entirely.

```yaml
- name: Skip build if image already at SHA
  id: cache_check
  run: |
    if curl -sI "https://ghcr.io/.../continuum-core:${SHORT_SHA}" -H "Authorization: Bearer ${TOKEN}" | head -1 | grep -q "200"; then
      echo "skip=true" >> "$GITHUB_OUTPUT"
    fi
- name: Build
  if: steps.cache_check.outputs.skip != 'true'
  run: bash scripts/push-current-arch.sh
```

Also: cache `Cargo.lock` content-hash → image-SHA mapping in a small registry-side metadata file so even repeat-rebuilds across PRs reuse images.

**Why.** Cuts CI burn by ~80% for repeat-rebuilds (especially during stack-of-PRs cycles where the same Rust core is referenced across multiple PRs).

**Done when**: a no-op PR that doesn't change Cargo.lock OR Dockerfile reuses the previous image; build job time < 30s for the cache-hit path.

### Phase F — airc-side observability + capability publication

**What.** Each self-hosted runner publishes its online state + capability on the `#ai-capability` airc channel (per AGENT-BACKBONE §4.3). The continuum orchestrator subscribes to this channel + can see which runners are online.

Optional next layer: when a PR opens that requires Docker builds AND no suitable runner is online, the orchestrator (or a meta-coordinator agent) DM's the appropriate hardware owner via airc to ask them to wake the runner.

**Why.** Folds the build farm into the same mesh-observability layer the rest of the system uses. Same airc channel humans use to coordinate; runners become first-class peers.

**Done when**: `airc capabilities` lists each online runner with its arch/GPU/role; the orchestrator can be queried for "is BigMama runner up?"; PR comment auto-posts "build-amd64 queued, BigMama offline — will start when it returns" if relevant.

## Risks + mitigations

- **Self-hosted runners need to stay online.** Mitigation: airc-side observability (Phase F) surfaces "runner offline" + the existing `airc daemon install` keeps runners up across machine sleep/wake (mirror of the airc#382 work).
- **Self-hosted runners get attack surface.** Mitigation: GHA's "require approval for first-time contributors" + the runners only run scripts already in the repo + airc-mesh contributors are gh-org members.
- **ghcr storage grows with every PR.** Mitigation: separate prune workflow that drops `:pr-<N>` tags after merge.
- **Phase A's auto-skip could mask real Docker bugs in Rust-only PRs.** Mitigation: the path filter is conservative — `core/**/Cargo.{toml,lock}` triggers the full path even for "small" Rust changes. False positives (running real verification when a Rust change actually had no Docker impact) are cheap; false negatives (skipping when a real check was needed) are tracked + the path-filter list is tightened over time as we observe.

## Action item: top-level GitHub issue

This doc is referenced from a top-level continuum GitHub issue that tracks each phase as a sub-task with its own PR + status. As phases land, sub-tasks are checked off; the parent issue stays open until Phase F lands. That way the full plan is visible to anyone landing on the issue tracker, not buried in this doc.

## Today's mesh-coordination context

This plan was authored as part of Joel's "coordinated parallelism" framing for today's session:

- **M5 dev tab** (continuum-b741): owns F4 (carl-killer IPC pool recovery) + #75 (persona output quality) — TS-side fixes
- **M5 QA tab** (continuum-b741, this doc's author): owns Phase A + this doc + the issue
- **M1 Carl-validator tab**: owns post-Phase-A install validation + reporting findings via airc
- **Joel**: owns Phase B (runner registration on the hardware boxes) + the canary ruleset call

This doc + the top-level issue formalize that division so the mesh has a shared reference for who's doing what + what depends on what.
