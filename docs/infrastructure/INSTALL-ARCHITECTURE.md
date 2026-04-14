# Install Architecture

## Problem

Continuum serves two very different install consumers:

- **Carl** — curl user. Runs `curl -fsSL .../install.sh | bash` from a fresh
  machine. ADHD, no time, skeptical, needs one password prompt max and a
  working widget within ~60 seconds. Never clones the repo; doesn't want
  to. Consumes pre-built docker images from `ghcr.io/cambriantech`.
- **Dev** — contributor. Clones the repo, runs `npm start`. Tolerates longer
  build times in exchange for host toolchain control, hot reload, bleeding
  edge. Uses docker optionally (hybrid) but mostly local build.

The two paths must share all non-fundamentally-different logic so that
dev testing a change locally is a load-bearing proof that Carl's install
works. Divergence = dev ships green and Carl hits an untested wall.

## Invariants

These are design rules that CANNOT be violated without breaking the model.
They map one-to-one onto memory rules:

1. **ONE canonical install script.** `src/scripts/install.sh` is it. Both
   paths invoke the same file. No duplication of shared logic.
2. **Modular sudos.** Each sudo-requiring action is its own self-guarded
   function. Re-runs that are a no-op prompt for NO passwords. Sudo
   warmup arms lazily from the first module that actually needs it.
3. **Carl's launch budget is ~60 seconds.** Pre-pulled images, constant
   progress output, one password prompt max (usually zero if nothing
   needs sudo this run). Widget opens ASAP; API-key entry is a post-it.
4. **Dev → Carl pipeline is CI images.** Dev's code reaches Carl through
   `.github/workflows/docker-images.yml` building and publishing the
   Dockerfiles to `ghcr.io`. Any image Carl needs must have a CI job.
5. **BigMama = staging.** Pre-merge e2e dry-run of the Carl path runs on
   BigMama (RTX 5090, Windows/WSL2, Tailscale-reachable). Matches Toby's
   Windows dogfood shape.
6. **Docker in chunks.** Multi-service compose, focused images, shared
   base layers. Carl pulls deltas, not monoliths.

## Module shape

Every install step is a function with this contract:

```bash
mod_<category>_<name>() {
  # STEP 1: Idempotency guard — return 0 if already satisfied.
  # This is the `if installed` check Joel keeps calling for.
  if <target_state_already_true>; then
    module_skip "<name>" "<reason>"   # one-line green log
    return 0
  fi

  # STEP 2: Applicability guard — return 0 if not needed on this platform/mode.
  <platform_or_mode_check> || return 0

  # STEP 3: Announce what we're about to do, in plain English.
  # Carl is skeptical — he needs to see exactly why we're about to touch
  # his machine.
  module_start "<name>" "Doing X because Y is missing"

  # STEP 4: Acquire sudo lazily if needed, and only if needed.
  if <needs_sudo>; then
    ensure_sudo_warmed  # idempotent; prompts at most once per install run
  fi

  # STEP 5: Do the work. Fail loud with a specific next-step message.
  <the_actual_work> \
    || module_fail "<name>" "Clear actionable error: try X, or re-run with Y"

  # STEP 6: Green log.
  module_done "<name>"
}
```

Four helper primitives the shell library exposes:

- `module_skip <name> <reason>` — logs `✓ name: reason`
- `module_start <name> <what>` — logs `⧗ name: what`
- `module_done <name>` — logs `✓ name`
- `module_fail <name> <fix>` — logs `✗ name — <fix>`, exits non-zero
- `ensure_sudo_warmed` — lazy sudo cache; first call prompts (if needed),
  subsequent calls are no-ops. Arms a 50s keepalive + EXIT trap on first
  successful prompt.

## Module catalog

Organized by category. `mode` column: **C** = Carl, **D** = Dev, **B** = both.

| Module | mode | Sudo? | Guard |
|---|---|---|---|
| `mod_os_detect` | B | no | always run |
| `mod_git_ensure` | B | linux: yes | `command -v git` |
| `mod_repo_clone_or_pull` | B | no | `.git` dir presence |
| `mod_submodules_init` | B | no | `test -f vendor/llama.cpp/CMakeLists.txt` |
| `mod_docker_reachable` | C + D-hybrid | no | `docker info` succeeds |
| `mod_docker_wsl_integration` | C (WSL2) | no | `/var/run/docker.sock` exists |
| `mod_continuum_bin_link` | C | yes | `test -x /usr/local/bin/continuum` |
| `mod_config_env` | B | no | `~/.continuum/config.env` exists |
| `mod_docker_pull_images` | C | no | `docker image inspect` for each service |
| `mod_docker_compose_up` | C | no | healthcheck passing on each service |
| `mod_open_widget` | C | no | `--no-open` flag absent |
| `mod_node` | D | no | `node --version` matches .nvmrc |
| `mod_rust` | D | no | `rustc --version` |
| `mod_python_ml_venv` | D | no | `~/.continuum/venv` present + requirements satisfied |
| `mod_system_deps` | D (linux) | yes | `dpkg -l` for needed packages |
| `mod_postgres_local` | D | yes | systemd unit or brew service state |
| `mod_livekit_local` | D | no | binary at expected path |
| `mod_tailscale_install` | D | yes | `command -v tailscale` |
| `mod_tls_certs` | C + D | no | cert file present |
| `mod_cargo_build` | D | no | binary freshness vs source mtime |
| `mod_npm_install` | D | no | node_modules integrity |
| `mod_npm_start` | D | no | no-op (exec, does not return) |

## Dispatch

Top of `src/scripts/install.sh`:

```bash
MODE="${CONTINUUM_INSTALL_MODE:-dev}"  # dev | carl | both
AUTO_LAUNCH="${CONTINUUM_AUTO_LAUNCH:-0}"

case "$MODE" in
  carl)
    mod_list=(
      mod_os_detect
      mod_git_ensure
      mod_repo_clone_or_pull
      mod_submodules_init
      mod_docker_wsl_integration
      mod_docker_reachable
      mod_continuum_bin_link
      mod_config_env
      mod_docker_pull_images
      mod_docker_compose_up
      mod_open_widget
    )
    ;;
  dev)
    mod_list=(
      mod_os_detect
      mod_submodules_init
      mod_system_deps
      mod_node
      mod_rust
      mod_python_ml_venv
      mod_postgres_local
      mod_livekit_local
      mod_tailscale_install
      mod_tls_certs
      mod_cargo_build
      mod_npm_install
    )
    [ "$AUTO_LAUNCH" = "1" ] && mod_list+=(mod_npm_start)
    ;;
  both)
    # Dev who also wants Carl's local docker images
    mod_list=(<union of dev and carl, deduplicated, docker path last>)
    ;;
esac

for mod in "${mod_list[@]}"; do
  "$mod"
done
```

## Call sites

Two entry points, both exec the same canonical script:

### Root `install.sh` (Carl's curl target)

Minimal bootstrap. Exists only because Carl can't run
`src/scripts/install.sh` without first having the repo.

```bash
#!/bin/bash
# Carl's entry point. Delegates to src/scripts/install.sh after clone.
set -e

# Modules that MUST run before the repo clone:
#   - OS detect (trivial, inline)
#   - git install (use mod_git_ensure from an inline minimal copy)
#   - clone
# Then exec the canonical script with carl mode.

REPO="https://github.com/CambrianTech/continuum.git"
INSTALL_DIR="${CONTINUUM_DIR:-$HOME/continuum}"

# (inline trivial OS detect + git install here; ~20 lines)

if [ ! -d "$INSTALL_DIR/.git" ]; then
  git clone --depth 1 "$REPO" "$INSTALL_DIR"
fi
cd "$INSTALL_DIR"
git submodule update --init --recursive

exec env CONTINUUM_INSTALL_MODE=carl bash src/scripts/install.sh
```

### `parallel-start.sh` (Dev's `npm start` path)

Already invokes `src/scripts/install.sh`. Sets `CONTINUUM_DEPS_ONLY=1`
today; switching to `CONTINUUM_INSTALL_MODE=dev` and honoring
`CONTINUUM_DEPS_ONLY` as a sub-mode that skips `mod_cargo_build` +
`mod_npm_install` gives us the same behavior without two flags.

## CI contract (the Dev → Carl bridge)

`.github/workflows/docker-images.yml` MUST build every Dockerfile that
any mode of the canonical install script tries to pull. Current gap:

- `docker/continuum-core.Dockerfile` — ✓ has a build job
- `docker/continuum-core-cuda.Dockerfile` — ✗ NO job, but
  `docker-compose.gpu.yml` tries to use the image. Carl's GPU path fails.

Required additions:

1. `continuum-core-cuda` build job (mirror the continuum-core job, different
   Dockerfile, publishes to `ghcr.io/cambriantech/continuum-core-cuda:latest`).
2. PR-gated smoke build for changed Dockerfiles. Not a full publish; uses
   registry cache, `push: false`, fails the PR if the image can't build.
3. Post-merge smoke boot: pull the published image, run it in a disposable
   container, hit its healthcheck. Catches "published but broken."

## Testing

The install script has three layers of verification:

1. **Unit-level** (each module individually):
   `CONTINUUM_INSTALL_RUN_MODULE=mod_docker_wsl_integration bash src/scripts/install.sh`
   runs a single module against current environment state.
2. **Integration** (dev path from a clean VM):
   Dev's own re-run on laptop after a working setup. If zero modules do
   work, zero prompts fire. That's the idempotency test.
3. **E2E** (Carl path from BigMama as staging):
   Fresh `curl | bash` from a staging Ubuntu container on BigMama. Must
   complete to widget-open within ~60s after pre-pull warmup, with at most
   one sudo prompt.

## Golden setup principle

The assertion Carl must pass every time, first run, any fresh machine:

1. `curl -fsSL .../install.sh | bash`
2. **EXACTLY ONE password prompt for the entire install run** — and only
   if at least one module actually needs sudo. No second prompt, no
   surprise mid-install prompt, not for a "different step," not ever.
   Re-runs that do no work prompt for zero passwords. This holds for
   Carl, Dev, AND Joel. Any second prompt is a bug, period.
3. Constant progress output; no silent dead-air stretches > 5 seconds.
4. Widget open in his browser within ~60 seconds.
5. First persona responds within another ~30 seconds (localized to the
   machine — no API keys required for the local Candle path).

Nothing else is acceptable. Every regression against this contract is a
bug, not a feature request. Every module in the catalog above must either
hit this budget or declare itself a Dev-only module.

### The one-prompt contract (load-bearing)

```
ensure_sudo_warmed() {
  # Already root? Nothing to do.
  [ "$(id -u)" -eq 0 ] && return 0
  # Passwordless sudo available? Nothing to do.
  sudo -n true 2>/dev/null && return 0
  # Already warmed THIS run? Nothing to do.
  [ "$_SUDO_WARMED" = "1" ] && return 0
  # Not a terminal? Cannot prompt — fail loud.
  [ -t 0 ] || module_fail "sudo" "No TTY for sudo prompt. Re-run in a terminal, or install via the docker-compose path which needs no sudo."

  info "Admin access needed — prompting once now; won't ask again this run."
  sudo -v || module_fail "sudo" "Password failed; re-run."

  # Arm keepalive for the rest of the script. Dies with parent via EXIT trap.
  ( while true; do sudo -n true 2>/dev/null || exit; sleep 50; done ) &
  _SUDO_KEEPALIVE_PID=$!
  trap '_sudo_cleanup' EXIT
  _SUDO_WARMED=1
}

_sudo_cleanup() {
  [ -n "$_SUDO_KEEPALIVE_PID" ] && kill "$_SUDO_KEEPALIVE_PID" 2>/dev/null || true
}
```

This is the ONLY legal place in the entire codebase where a sudo prompt
can originate. Every other sudo call assumes warming has happened (the
warming function is idempotent and lazy, so it's safe to call inside
modules). Any module that does `sudo <thing>` directly without first
calling `ensure_sudo_warmed` is broken.

**Assertion for CI / dev-test:** on a machine with warmed sudo (`sudo -v`
already run), a fresh `src/scripts/install.sh` run that needs sudo for
N modules must cause zero prompts. Verifiable by capturing stderr and
asserting absent of any "password" string.

## Future: Kubernetes friendliness (work in progress)

Continuum's target evolution is to run on K8s clusters for team / enterprise
deploys. The current docker-compose shape is intentionally a strict subset
of K8s — every design choice here should translate cleanly. Plan for it
without building it yet:

### Already K8s-friendly
- Each service is its own focused image with its own Dockerfile (maps 1:1
  to a Deployment).
- Healthchecks are defined per-service (→ `livenessProbe` / `readinessProbe`).
- Configuration comes from env + mounted files (→ `ConfigMap` / `Secret`).
- Data volumes are explicit + named (→ `PersistentVolumeClaim`).
- No service writes to a random shared directory of its neighbor.

### Needs attention before K8s
- **Cross-service IPC via Unix socket.** `continuum-core` and `node-server`
  today talk through `/root/.continuum/sockets/continuum-core.sock` on a
  shared volume. Two options when we go multi-pod:
  - (a) Same-pod sidecar with `emptyDir` shared volume — works today with
    zero code change, but pins the two services to the same node.
  - (b) Switch the IPC transport to TCP/gRPC on a Service DNS name — more
    work, more flexibility, needed once the two need independent scaling.
  - Decision deferred; plan the switch so it's a single-PR flip, not a
    rewrite. Keep the RustCoreIPCClient's transport behind an interface
    that can swap `unix://` for `tcp://` with config.
- **Absolute filesystem paths.** `$HOME/.continuum/...` works for docker
  compose via a bind mount but is fragile in K8s pods. Migrate callers
  to read from a single configured root (`CONTINUUM_STORAGE_PATH` env),
  which maps to `/app/storage` in containers and is a PVC in K8s.
- **Postgres connection via DNS.** Today the compose has `postgres:5432`
  as the hostname — fine for compose, fine for K8s if we name the Service
  `postgres`. Don't hardcode `localhost` anywhere.
- **Tailscale / networking.** Carl's Tailscale-based TLS flow doesn't
  generalize to a K8s Service. For K8s, TLS comes from cert-manager +
  Ingress. The install script's Tailscale step is Carl-path-only; K8s
  path has its own TLS workflow.
- **The single install script does NOT install K8s.** A K8s install is a
  Helm chart (future), not a bash module. `src/scripts/install.sh` remains
  the Carl + Dev path. The Helm chart consumes the same published
  ghcr.io images — that's the unification point.

### Immediate implication for this PR
- Nothing here blocks #891. The constraint is: don't ADD cross-service
  filesystem assumptions beyond the one shared socket directory that
  already exists. Any new service must talk over IPC/network, not via
  baked-in paths.

## Adding a new module (extensibility contract)

When a future need drops — new dep, new service, new auth step, new cloud
provider, new database — the answer is **always** "add another module."
The module's contract is fixed; that's what makes the architecture
survive change.

**Checklist for authors of a new module:**

1. **Name it** `mod_<category>_<name>`. One verb per module — install,
   ensure, link, enable. If it does more than one thing, split it.
2. **Declare its mode** in the module catalog table. Carl-only, Dev-only,
   or both. Document *why* on whichever side it isn't.
3. **Write the idempotency guard first.** What does the "done" state look
   like? Express it as a file presence / command-output / config flag
   check. Re-running the module must be a no-op when done.
4. **Write the applicability guard.** Does this only apply on WSL2? on
   NVIDIA hosts? on fresh installs? Skip early on "not applicable."
5. **Announce in plain English.** `module_start "name" "X because Y"`.
   Carl is skeptical — he reads the output to decide whether to trust us.
6. **If sudo is needed, call `ensure_sudo_warmed` INSIDE the module.**
   Never at the top of the script. Never speculatively.
7. **Fail loud with a fix.** `module_fail "name" "<clear next step>"`.
   "Docker Desktop isn't running — start it and re-run" is correct. Silent
   skip is never correct.
8. **Update CI if the module implies a new image.** If your module does
   `docker pull X`, then CI must build + publish X.
9. **Dry-run it on BigMama** against the Carl path before merging.

**The "real life scenario" validation:** the WSL Docker Desktop
integration fix IS the living test of this architecture. It's a new-dep
/ new-dependency-hazard that a user hits at install time. The architecture
handles it by:

- Making it a module (`mod_docker_wsl_integration`).
- Self-guarding on `/var/run/docker.sock` presence (idempotent).
- Applicability-guarding on WSL2 + Docker Desktop shared mount presence.
- Requiring no Linux sudo (edits Windows-owned AppData; bounces Docker
  Desktop via powershell.exe).
- Announcing "Docker Desktop detected; ensuring WSL integration for '$distro'…"
- Failing loud if the socket doesn't materialize within 60s, with the
  next step being "run `wsl --shutdown` from Windows PowerShell and re-run."

A hypothetical future new-dep (new database, new runtime, new auth
provider) slots into the same contract without touching the dispatcher
or the shared-primitive library. New row in the catalog, new `mod_*`
function, new CI job if it ships an image. Nothing else changes.

## Mapping back to the current PR #891

Already landed on the branch (OK against this architecture):
- `29f526670` — Dockerfile fail-fast for submodules. Stays as a backstop
  even after `mod_submodules_init` runs in install.
- `830db4ce4` — build.rs arm64 FP16 fix. Out-of-scope for install.sh; in
  scope for the Rust substrate.
- `b72782ece` — `docker-compose.gpu.yml` override. Expects
  `continuum-core-cuda` image to exist (CI gap).

To do before merge (consolidation):
- Move WSL fix + submodule init from root `install.sh` to
  `src/scripts/install.sh` as modular steps.
- Collapse root `install.sh` to the bootstrapper pattern above.
- Add `continuum-core-cuda` CI job.
- Add PR-gated smoke build in the workflow.
- BigMama e2e dry-run documenting results in PR description.
