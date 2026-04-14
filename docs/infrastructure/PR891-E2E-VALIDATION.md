# PR #891 — BigMama E2E Validation Playbook

Pre-merge dry-run for `feat(inference): own the substrate — vendored llama.cpp +
continuous-batching scheduler + cross-machine chat`.

Per [INSTALL-ARCHITECTURE.md](INSTALL-ARCHITECTURE.md) invariant 5: BigMama
(RTX 5090, Windows/WSL2, Tailscale-reachable) is the staging target. This run
must pass before #891 flips ready-for-review.

## What this PR changes that this validation must catch

- New `continuum-core-cuda` CI image (was orphaned). `docker compose --profile
  gpu` now pulls it from ghcr instead of failing.
- `docker-compose.gpu.yml` override wires the cuda image as `continuum-core`
  under the gpu profile, shadows the upstream `inference` (llama-server) service.
- `workers/llama/build.rs` arm64 FP16 fix (irrelevant on BigMama amd64; already
  validated on M5).
- Continuous-batching scheduler with shared `Context`, `n_seq_max=3`,
  `n_ctx = context_length × n_seq_max`. Personas now flow through this
  scheduler over the local Unix socket — NOT the upstream llama-server HTTP.
- Single-source-of-truth concurrency: `system_resources::local_inference_capacity()`
  in Rust, surfaced via `inference/capacity` IPC, consumed by TS
  `InferenceCoordinator`.

## Pre-flight (Joel)

- [ ] Docker Desktop on BigMama: WSL2 integration enabled for default distro
      (the wall the rest of this is gated on).
- [ ] `~/.continuum/config.env` on BigMama has at least one cloud API key
      (Anthropic/OpenAI/Groq) so non-CUDA personas can also activate as
      regression sanity check.
- [ ] `green` user has repo at `/home/green/continuum` (or fresh-clone target).

## Path A — Docker / GPU profile (the substrate-on-CUDA proof)

Owner: shared (memento drives, I review). This is the path that proves the
new scheduler actually decodes on the 5090.

```bash
cd /home/green
rm -rf continuum-e2e && git clone https://github.com/CambrianTech/continuum.git continuum-e2e
cd continuum-e2e
git checkout feature/inference-perf
git submodule update --init --recursive   # mod_submodules_init covers this for Carl

# Pull cuda image from ghcr (validates CI built + published it):
docker compose -f docker-compose.yml -f docker-compose.gpu.yml --profile gpu pull continuum-core
docker compose -f docker-compose.yml -f docker-compose.gpu.yml --profile gpu up -d

# Wait for boot
sleep 30
docker ps --filter "name=continuum-core"
```

### Smoke checklist

- [ ] `docker ps` shows `continuum-core` running with `runtime: nvidia`.
- [ ] `docker exec continuum-core nvidia-smi` returns the 5090.
- [ ] Socket `/root/.continuum/sockets/continuum-core.sock` exists inside
      the container (`docker exec continuum-core test -S /root/.continuum/sockets/continuum-core.sock`).
- [ ] Logs show ggml-cuda backend initialized, NOT a fallback to CPU:
      `docker logs continuum-core 2>&1 | grep -iE "cuda|gpu|metal|fallback"`.
- [ ] Logs show **OUR** scheduler (not upstream llama-server):
      `docker logs continuum-core 2>&1 | grep -iE "scheduler|seq_id|n_seq_max"`
      should match; `grep -i "llama-server"` should NOT.
- [ ] `docker compose ps inference` shows `replicas: 0` (the gpu override
      shadowed the legacy upstream service — no port :8090 listener).

### Live persona inference

- [ ] Open the widget URL (Tailscale-reachable host of BigMama widget-server).
- [ ] Send a message in `general` room — local persona (Helper AI / Local
      Assistant) should respond within 10s.
- [ ] During the response, `nvidia-smi` (host or `docker exec`) shows
      utilization > 0% on the 5090. Capture the snapshot.
- [ ] Send 3 messages back-to-back rapidly — verify the scheduler batches
      them (look for multi-seq batch lines in logs, not serial decode).

## Path B — Carl install (curl-fresh)

Owner: memento. Validates `mod_submodules_init`, `mod_docker_wsl_integration`,
the one-prompt sudo contract.

```bash
# In a fresh user home with NO continuum repo:
sudo -K   # clear cached sudo timestamp; force a fresh prompt
curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/feature/inference-perf/install.sh | bash
```

### Smoke checklist

- [ ] Exactly **one** sudo password prompt total across the install (the
      `ensure_sudo_warmed` invariant).
- [ ] If WSL integration disabled, install errors with the documented hint
      (`mod_docker_wsl_integration` should detect + print Docker Desktop
      toggle path).
- [ ] After install completes, `docker compose ps` shows the same services
      as Path A.
- [ ] Widget loads at the expected URL within ~60s end-to-end (Carl's
      launch budget per invariant 3).

## Path C — Dev install (clone + npm start)

Owner: shared. Sanity check that the same modules work in dev mode.

```bash
git clone https://github.com/CambrianTech/continuum.git
cd continuum/src
npm start  # parallel-start.sh delegates to src/scripts/install.sh --mode=dev
```

### Smoke checklist

- [ ] Same one-prompt sudo behavior as Path B.
- [ ] `npm start` completes Rust + TS build, opens browser to widget.
- [ ] `./jtag ping` succeeds.
- [ ] Local persona responds in `general` (uses the local-Metal/CPU stub
      adapter on BigMama unless we forced CUDA — verify which by checking
      the persona's adapter log).

## Artifact capture (for PR description)

Capture these and paste links/screenshots into the PR body:

1. `docker logs continuum-core` (gzipped, full boot to first persona response)
2. `nvidia-smi` snapshot during inference
3. Browser screenshot of widget with persona response visible
4. `docker compose -f docker-compose.yml -f docker-compose.gpu.yml --profile gpu config`
   (final resolved compose, proves the override layered correctly)
5. Time-to-first-token measurement (`grep "Generated.*tok/s" docker logs continuum-core`)

## Stop conditions (don't merge if)

- Path A's `nvidia-smi` shows zero GPU utilization during inference (CPU fallback).
- Path A logs contain `llama-server` references (legacy service still routing).
- Path B issues more than one sudo prompt for a single install.
- Any path fails to bring up a working widget within 5 minutes of the docker pull.

## Known gaps not gating this run

- arm64 docker on BigMama (BigMama is amd64; arm64 path validated on M5 already).
- LoRA hot-swap per-request (scheduler v1 limitation; tracked separately).
- LiveKit/avatars (broken by separate sensory-transport split work, not by #891).
