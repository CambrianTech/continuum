#!/bin/bash
# Continuum — One-command installer
# Usage: curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/install.sh | bash
#
# NOTE (2026-08-07): this used to advertise https://cambriantech.github.io/continuum/install.sh.
# That URL 404s — GitHub Pages is not published for this repo — so the ONE path we told
# 'most users' to take piped a 9 KB GitHub 404 page into their shell. Verified with curl.
# Points at the raw ref that actually serves. If Pages is ever published, change it back
# HERE and in tools/scripts/install.sh, and verify with an HTTP status, not by reading.
#
# Docker-first: pulls pre-built images, no compilation needed.
# Optional: Tailscale for mesh networking + TLS (voice/video).
set -e

# Log primitives (info/ok/warn/fail/die) come from
# tools/scripts/lib/install-common.sh after clone. Until the repo is
# cloned, use these minimal pre-clone versions; they'll be overridden
# when we source the canonical library below.
info()  { echo -e "\033[1;36m→\033[0m $*"; }
ok()    { echo -e "\033[1;32m✓\033[0m $*"; }
warn()  { echo -e "\033[1;33m!\033[0m $*"; }
fail()  { echo -e "\033[1;31m✗\033[0m $*"; exit 1; }
# Alias so the canonical lib's `die` also works here and vice versa.
die()   { fail "$@"; }

REPO="https://github.com/CambrianTech/continuum.git"
INSTALL_DIR="${CONTINUUM_DIR:-$HOME/continuum}"
CONTINUUM_DATA="$HOME/.continuum"

# ── Friendly-failure infrastructure ─────────────────────────
# When install.sh fails partway, Carl needs to know WHICH phase died,
# not just what bash printed. PHASE gets updated as we enter each
# section; the ERR trap reads it + maps to phase-specific guidance.
# Empirically (2026-04-25): existing failures dump bash's last line
# of stderr with no context. Carl can't tell if it's a Docker thing,
# a Tailscale thing, a model-download thing, or a Rust build thing
# without reading install.sh source.
PHASE="(starting up)"
INSTALL_LOG="${INSTALL_LOG:-/tmp/continuum-install-$$.log}"
exec > >(tee -a "$INSTALL_LOG") 2>&1

phase_guidance() {
  case "$PHASE" in
    *"detect environment"*) echo "Verify uname -s + uname -m return expected values; check disk space (df -h /).";;
    *"pre-clone bootstrap"*) echo "Install git + docker first; on Mac, ensure Docker Desktop is running.";;
    *"clone"*|*"update repo"*) echo "Check network: ping github.com; verify INSTALL_DIR ($INSTALL_DIR) is writable.";;
    *"shared modules"*) echo "Re-clone may be incomplete; rm -rf $INSTALL_DIR && re-run installer.";;
    *"configuration"*) echo "Check $CONTINUUM_DATA exists + is writable; mkdir -p $CONTINUUM_DATA && chmod 700 $CONTINUUM_DATA.";;
    *"TLS certs"*) echo "Tailscale + cert step is optional; export CONTINUUM_NO_TLS=1 and re-run.";;
    *"compose files"*) echo "Verify docker-compose.yml exists in $INSTALL_DIR; the install repo may be incomplete.";;
    *"pull"*|*"images"*) echo "Network or GHCR auth issue; docker login ghcr.io and retry.";;
    *"start support services"*|*"bring up"*) echo "Check Docker Desktop has enough RAM (≥30GB). docker compose -f $INSTALL_DIR/docker-compose.yml logs --tail=100";;
    *"widget-server health"*) echo "Compose came up but widget-server isn't serving. docker compose -f $INSTALL_DIR/docker-compose.yml logs widget-server --tail=100";;
    *) echo "Capture full log + open an issue: cat $INSTALL_LOG | gh issue create -t 'install fail @ $PHASE' -b -";;
  esac
}

on_install_fail() {
  local rc=$?
  # Trap fires on any non-zero exit (set -e). Avoid recursing if the
  # ERR trap itself trips a sub-shell.
  trap - ERR EXIT
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  ❌ Install failed during phase: $PHASE  (exit $rc)"
  echo ""
  echo "  Suggestion: $(phase_guidance)"
  echo ""
  echo "  Full log: $INSTALL_LOG"
  echo "  Last 30 lines:"
  tail -30 "$INSTALL_LOG" | sed 's/^/    /'
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  exit "$rc"
}
trap on_install_fail ERR

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Continuum Installer"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Log: $INSTALL_LOG"
echo ""

# ── 1. Detect environment ───────────────────────────────────
PHASE="detect environment"
info "Detecting environment..."

OS="$(uname -s)"
ARCH="$(uname -m)"
HAS_GPU=false

case "$OS" in
  Linux)
    if command -v nvidia-smi &>/dev/null || [ -f /usr/lib/wsl/lib/nvidia-smi ]; then
      HAS_GPU=true
      GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1 || echo "NVIDIA GPU")
      ok "GPU detected: $GPU_NAME"
    fi
    ;;
  Darwin)
    ok "macOS $ARCH"
    ;;
  *) fail "Unsupported OS: $OS" ;;
esac

# ── 2. Pre-clone bootstrap: git + minimal Docker presence check ────
PHASE="pre-clone bootstrap"
# We can't source the canonical module library yet (lives in the repo).
# Just verify prerequisites so the clone can happen. Deeper checks live
# in the canonical modules that run after the clone.

if ! command -v git &>/dev/null; then
  case "$OS" in
    Linux) fail "git required. Run: sudo apt-get install -y git  (or equivalent), then re-run." ;;
    Darwin) fail "git required. Run: brew install git  (or install Xcode CLI tools), then re-run." ;;
  esac
fi

# Container runtime + inference setup.
#
# Linux: Docker Engine + continuum-core runs containerized (with cuda/vulkan
# GPU passthrough via /dev/dri or runtime:nvidia). Everything in containers.
#
# Mac: Docker Desktop for support services ONLY. continuum-core runs NATIVELY
# on the host to access Metal for Candle embeddings, Bevy headless avatar
# render, vision processing, and audio MPS paths — Apple's hypervisor exposes
# no GPU to containers (Docker themselves confirmed in Feb 2026), so anything
# Metal-needing must be on the host. LLM inference routes to Docker Model
# Runner's vllm-metal backend, which also runs native on the host — Docker
# Desktop manages the process but the compute happens on Apple Silicon directly.
#
# CONTAINER_CMD is used for every `compose` / `info` call. On Mac that
# handles support services; continuum-core-server is launched separately
# as a native host process via `npm start`.
case "$OS" in
  Linux)
    if ! command -v docker &>/dev/null; then
      info "Docker not found — installing via get.docker.com…"
      curl -fsSL https://get.docker.com | sh
      sudo usermod -aG docker "$USER"
      warn "Added $USER to docker group — log out and back in, then re-run this script"
      exit 0
    fi
    CONTAINER_CMD=docker
    ;;
  Darwin)
    if ! command -v docker &>/dev/null; then
      fail "Docker Desktop required on Mac.

  1. Download: https://docker.com/products/docker-desktop  (4.62+ for Model Runner)
  2. Install the .dmg, then launch Docker Desktop from Launchpad
  3. When prompted, grant Admin password for the vmnetd privileged helper
     (one-time macOS permission for container networking — standard Docker setup)
  4. Wait for the whale icon in your menu bar to show 'Docker Desktop is running'
  5. Re-run this install script
"
    fi
    if ! docker info &>/dev/null 2>&1; then
      fail "Docker Desktop is installed but not running.

  1. Launch Docker Desktop from Launchpad (or Applications)
  2. If this is your first launch, macOS will prompt for Admin password for the
     vmnetd privileged helper — click 'Allow' and enter your password
  3. Wait for the whale icon in your menu bar to show 'Docker Desktop is running'
  4. Re-run this install script

  (Scripted \`open -a Docker\` can't satisfy the macOS privileged-helper prompt —
   that's why this script asks you to launch Docker Desktop manually once.)
"
    fi
    # ── Docker Desktop VM memory (Mac Option B — continuum-core NATIVE) ─────
    # The previous 80%-of-RAM target crashed Docker Desktop mid-run on 32GB
    # M1 during matrix testing (<external-drive> 2026-04-16): Docker VM at 25.6GB
    # + native continuum-core at ~11GB RSS + macOS overhead ~6GB ≈ 43GB on a
    # 32GB physical box → heavy swap → Docker daemon died, DMR endpoint
    # disappeared, Helper AI fell back to Candle (5x slower) and never
    # produced a reply within the chat gate. Real-world blocker on the
    # primary-audience hardware.
    #
    # Mac Option B has THREE concurrent memory tenants, not two:
    #   (a) native continuum-core     ~12GB budget (Qwen 4B Q4 + KV + Candle
    #                                  embeddings + Bevy render + vision +
    #                                  audio + working set)
    #   (b) Docker Desktop VM         hosts postgres*, node-server, widget-
    #                                  server, livekit-bridge, model-init.
    #                                  With SQLite default (postgres opt-in),
    #                                  the active containers need ~6-10GB.
    #   (c) macOS itself              kernel, window server, user apps     ~6GB
    #
    # So Mac Option B target: PHYS - NATIVE_BUDGET(12) - MACOS_OVERHEAD(6)
    # = PHYS - 18GB headroom reserve. Floor at 10GB (below that, containers
    # don't fit; Option B isn't viable on that hardware).
    #
    # Physical memory sizing (Option B honest mins, not aspirational):
    #   32GB  → 14GB Docker VM (comfortable)
    #   24GB  →  6GB Docker VM (below floor → refuse)
    #   Below 24GB → refuse install (can't fit all three tenants).
    PHYS_BYTES=$(sysctl -n hw.memsize)
    PHYS_MIB=$((PHYS_BYTES / 1048576))
    PHYS_GB=$((PHYS_MIB / 1024))

    # Hardware tier — sets NATIVE_RESERVE + PERSONA_MODEL to fit available RAM.
    # Per Joel's "MacBook Air on up, accessible, high-school-computer" target:
    # 16GB MBA must be a working OOTB chat experience, not a 28GB-floor reject.
    # Tier breakdown (continuum-ai's published smaller models all public):
    #   8-15GB  → reject; even minimal config doesn't fit (macOS 6GB +
    #             Docker 4GB minimum + minimal continuum-core 3GB + small
    #             model + working set ≈ 14-15GB working set, no headroom)
    #   16-23GB → MBA tier: smaller persona model, no Bevy/vision/audio
    #             pre-pull at install time (chat-only OOTB; multimodal
    #             enables when user attaches an image / opens video chat —
    #             those code paths still load lazily). Native budget 5GB.
    #   24-31GB → mid tier: still chat-focused but slightly larger model;
    #             Bevy/vision/audio available. Native budget 8GB.
    #   32GB+   → primary tier: full Qwen 4B code-forged + multimodal +
    #             everything pre-pulled. Native budget 12GB (original).
    #
    # PERSONA_MODEL also tiers (set later when ic_decide_gpu_path runs;
    # this just sets the byte budget for Docker VM sizing). The tiered
    # PERSONA_MODEL is referenced by the docker model pull section below.
    if [[ "$PHYS_MIB" -lt $((16 * 1024)) ]]; then
      fail "This Mac has ${PHYS_GB}GB physical RAM. Continuum's minimum is 16GB:
  - macOS itself reserves ~6GB
  - Docker Desktop VM needs at least ~4GB
  - Native continuum-core needs at least ~3GB (smallest persona model + working set)
  - Total minimum: 13-15GB, leaves no headroom under 16GB
For 16GB MBA: chat-only OOTB works (smaller model). For 32GB+: full multimodal experience."
    elif [[ "$PHYS_MIB" -lt $((24 * 1024)) ]]; then
      # MBA tier
      NATIVE_RESERVE_MIB=$((5 * 1024))
      CONTINUUM_TIER="mba"
      info "Hardware tier: MBA (${PHYS_GB}GB) — chat-only OOTB with smaller persona model"
    elif [[ "$PHYS_MIB" -lt $((32 * 1024)) ]]; then
      # Mid tier
      NATIVE_RESERVE_MIB=$((8 * 1024))
      CONTINUUM_TIER="mid"
      info "Hardware tier: mid (${PHYS_GB}GB) — multimodal available with mid-size persona model"
    else
      # Primary tier (original behavior)
      NATIVE_RESERVE_MIB=$((12 * 1024))
      CONTINUUM_TIER="primary"
      info "Hardware tier: primary (${PHYS_GB}GB) — full multimodal + Qwen 4B code-forged"
    fi

    # Mac Intel override — RAM-based tier alone misclassifies Mac Intel +
    # discrete AMD or integrated Intel UHD as full/primary, but the
    # llama.cpp Metal-AMD shader path produces incoherent tokens on this
    # hardware (continuum 2026-05-30 evidence on MacBookPro15,1 / Radeon
    # Pro 560X: 0.8 tok/s + multilingual garbage + hundreds of nil
    # tensor buffer errors). Force the small CPU-runnable model tier
    # regardless of RAM until our CambrianTech/llama.cpp fork patches
    # the Metal-AMD kernels OR grid-share routes to an Apple-Silicon /
    # NVIDIA peer. Mirrors the Rust HwCapabilityTier::MacIntelMetalDiscrete
    # branch and the `mac_intel_discrete` tier in src/shared/models.json.
    CPU_BRAND=$(sysctl -n machdep.cpu.brand_string 2>/dev/null || echo "")
    if [[ "$CPU_BRAND" == *"Intel"* ]]; then
      info "Mac Intel detected ($CPU_BRAND) — overriding to mac_intel_discrete tier (Metal-AMD shaders unreliable; smallest forged model + CPU-only floor)"
      CONTINUUM_TIER="mac_intel_discrete"
      NATIVE_RESERVE_MIB=$((5 * 1024))
    fi
    export CONTINUUM_TIER
    MACOS_RESERVE_MIB=$((6 * 1024))
    HEADROOM_MIB=$((NATIVE_RESERVE_MIB + MACOS_RESERVE_MIB))
    DOCKER_FLOOR_MIB=$((4 * 1024))

    TARGET_MIB=$((PHYS_MIB - HEADROOM_MIB))
    if [[ "$TARGET_MIB" -lt "$DOCKER_FLOOR_MIB" ]]; then
      TARGET_MIB=$DOCKER_FLOOR_MIB
    fi

    CURRENT_MIB=$(docker system info --format '{{.MemTotal}}' 2>/dev/null | awk '{printf "%d\n", $1/1048576}')
    SETTINGS_FILE="$HOME/Library/Group Containers/group.com.docker/settings-store.json"
    # Bump if current is substantially below target (>10% gap — don't thrash
    # restarts over rounding noise).
    if [[ -f "$SETTINGS_FILE" ]] && [[ -n "$CURRENT_MIB" ]] && [[ "$CURRENT_MIB" -lt "$((TARGET_MIB * 90 / 100))" ]]; then
      info "Docker Desktop VM memory is ${CURRENT_MIB}MiB; bumping to ${TARGET_MIB}MiB (80% of ${PHYS_GB}GB host RAM, 16GB floor) for Continuum's inference + sensory workload…"
      python3 - <<PYEOF
import json, os
p = os.path.expanduser("$SETTINGS_FILE")
with open(p) as f:
    d = json.load(f)
d["MemoryMiB"] = $TARGET_MIB
with open(p, "w") as f:
    json.dump(d, f, indent=2)
PYEOF
      info "Restarting Docker Desktop to apply memory limit…"
      docker desktop restart >/dev/null 2>&1 || true
      # Wait for daemon to come back
      for _ in $(seq 1 30); do
        if docker info &>/dev/null 2>&1; then break; fi
        sleep 4
      done
      if ! docker info &>/dev/null 2>&1; then
        fail "Docker Desktop didn't come back after memory-limit restart. Launch it manually from Launchpad."
      fi
      NEW_MIB=$(docker system info --format '{{.MemTotal}}' 2>/dev/null | awk '{printf "%d\n", $1/1048576}')
      ok "Docker Desktop VM memory now ${NEW_MIB}MiB (target ${TARGET_MIB}MiB)"
    elif [[ -n "$CURRENT_MIB" ]]; then
      ok "Docker Desktop VM memory already ${CURRENT_MIB}MiB (≥ ${TARGET_MIB}MiB target)"
    fi

    # Docker Model Runner provides host-native vllm-metal for LLM inference.
    # Ships with Docker Desktop 4.62+. If `docker model` isn't available the
    # user's Docker Desktop is too old.
    if ! docker model --help &>/dev/null 2>&1; then
      fail "Docker Model Runner not available (needs Docker Desktop 4.62+).

  1. Open Docker Desktop → Settings → Software Updates → Check for updates
  2. Install the update (restart Docker Desktop if prompted)
  3. Re-run this install script
"
    fi
    # Verify the vllm runner is registered. On Docker DESKTOP (Mac), the
    # runners are bundled — Docker Desktop installs them automatically when
    # Model Runner is enabled. There's no /usr/local/cli-plugins step
    # (that's the Docker ENGINE / Linux path; `docker model install-runner
    # --help` says "Docker Engine only"). The earlier mkdir + install-runner
    # block was misapplied Linux logic on Mac, and forced a sudo prompt
    # for a directory Docker Desktop never reads from. Caught when CONTINUUM_
    # DEPS_ONLY=1 from parallel-start.sh tripped the prompt non-interactively
    # on every `npm start` (2026-04-16).
    #
    # If vllm shows "Not Installed", the user needs to enable it in Docker
    # Desktop → Settings → Beta features → Model Runner → install backends.
    # No CLI command can do this on Desktop, so we point at the GUI.
    if ! docker model status 2>/dev/null | awk '/^vllm[[:space:]]+Running/{found=1} END{exit !found}'; then
      warn "vllm-metal backend not registered with Docker Model Runner.
  Open Docker Desktop → Settings → Features in development → Model Runner
  → ensure 'Enable Docker Model Runner' is on → install the vllm backend.
  Continuum will fall back to llama.cpp until vllm is enabled (~5x slower
  on M-series for some models)."
    fi
    # Enable Model Runner's host-side TCP endpoint on port 12434. Without this,
    # continuum-core (running natively on the Mac host) can't reach the OpenAI-
    # compatible API — the probe in ai_provider.rs fails, the
    # docker-model-runner adapter doesn't register, and Candle becomes the
    # default local provider. That's a 5x perf regression (~10 tok/s vs ~50
    # tok/s on M5). Caught during M5 validation 2026-04-16: I had to enable
    # this manually before the adapter probe succeeded. Make it part of the
    # install so Carl never has to discover the toggle.
    #
    # `docker desktop enable model-runner --tcp=12434 --cors=all` is idempotent
    # — safe to re-run on every install. CORS=all is fine because the endpoint
    # binds 127.0.0.1 only (not exposed externally).
    if ! curl -fsS --max-time 1 http://localhost:12434/engines/llama.cpp/v1/models >/dev/null 2>&1; then
      info "Enabling Docker Model Runner TCP endpoint on localhost:12434…"
      docker desktop enable model-runner --tcp=12434 --cors=all 2>&1 | tail -3 || \
        warn "Could not enable Model Runner TCP — continuum-core will fall back to Candle (slower). Enable manually: docker desktop enable model-runner --tcp=12434 --cors=all"
    fi
    # cmake — required by the vendored llama.cpp build (Phase 2a of `npm
    # start`). Carl's M1 install pass (#980 Bug 1) hit
    #   thread 'main' panicked at cmake-0.1.57/src/lib.rs:1132:5:
    #   failed to execute command: No such file or directory (os error 2)
    #   is `cmake` not installed?
    # because install.sh said "✅ Continuum Tower installed!" without
    # checking cmake, then npm start died inside the cargo build of the
    # llama crate. Auto-install via brew matches the node pattern below
    # so fresh-Mac users have a working build path out of the box.
    if ! command -v cmake &>/dev/null; then
      if command -v brew &>/dev/null; then
        info "cmake not found — installing via Homebrew (needed by vendored llama.cpp build)…"
        brew install cmake
      else
        fail "cmake required for vendored llama.cpp build. Install Homebrew + run 'brew install cmake', or use 'xcode-select --install' to get the macOS CLI tools that include cmake."
      fi
    fi
    # Rust toolchain — continuum-core-server is built natively on Mac (not
    # containerized) so it can link Metal for Candle embeddings, Bevy, vision,
    # and audio MPS paths. Build happens during `npm start` at end of install.
    if ! command -v cargo &>/dev/null; then
      info "Rust not found — installing via rustup (needed for native continuum-core build)…"
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
      # shellcheck disable=SC1091
      [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
    fi
    # Node toolchain — required to build TypeScript side + run launcher.
    if ! command -v node &>/dev/null; then
      if command -v brew &>/dev/null; then
        info "Node not found — installing via Homebrew…"
        brew install node
      else
        fail "Node.js required. Install from https://nodejs.org or via Homebrew, then re-run."
      fi
    fi
    CONTAINER_CMD=docker
    ;;
esac

# ── GPU detection + persona model pull (shared by Carl + Dev) ──────
# Uses the centralized ic_detect_hardware / ic_decide_gpu_path from
# install-common.sh so both install.sh paths use the SAME detection.
# After this block: IC_GPU_PATH tells us which inference backend to
# verify, and the default persona model is pulled into DMR (if DMR path).
if type ic_detect_hardware &>/dev/null; then
  ic_detect_hardware
  ic_decide_gpu_path
  ic_describe_hardware

  # Hard-fail on unsupported. Previously this case fell through silently:
  # install.sh "completed", continuum runtime then errored on missing models.
  # That's the silent-failure-is-failure rule — Carl deserves an actionable
  # error at install time, not a confusing model-not-found at first chat.
  if [ "$IC_GPU_PATH" = "unsupported" ]; then
    cat >&2 <<EOF

ERROR: Continuum can't auto-detect a supported GPU path on this machine.
  Detected:  IC_PLATFORM=$IC_PLATFORM, IC_GPU_KIND=$IC_GPU_KIND
  Supported: macos:metal, linux:cuda, linux:rocm, linux:vulkan,
             wsl:cuda, wsl:vulkan, windows:cuda, windows:vulkan

If your hardware IS one of those, the detector missed something. Check:
  - macOS: 'sysctl -n machdep.cpu.brand_string' should mention "Apple"
  - Linux/WSL CUDA: 'nvidia-smi' should print GPU info
  - Linux ROCm: 'rocminfo' should print GPU info
  - Linux/WSL/Windows Vulkan: 'vulkaninfo --summary' should list deviceName
  - Windows CUDA: 'nvidia-smi' (Windows native) should print GPU info

If your hardware truly isn't supported, Continuum can't run reliably here.
File an issue at https://github.com/CambrianTech/continuum/issues with the
output of: uname -a + nvidia-smi (if installed) + vulkaninfo --summary.
EOF
    exit 1
  fi

  # Pull default persona model into DMR so Carl's first chat is instant.
  # Only for DMR paths — Vulkan path loads models differently (local GGUF).
  #
  # Tiered by CONTINUUM_TIER (set in the Mac RAM-tier block above; Linux
  # paths skip this block since CONTINUUM_TIER isn't set there → defaults
  # to the primary model). Lets a 16GB MBA install with a model that fits
  # rather than failing the install or OOMing on first chat.
  case "${CONTINUUM_TIER:-primary}" in
    mba)
      # 16-23GB: 0.8B general (~500MB GGUF). Chat-functional + leaves
      # headroom for macOS + Docker + native continuum-core working set.
      PERSONA_MODEL="hf.co/continuum-ai/qwen3.5-0.8b-general-forged"
      info "Persona model tier: MBA → qwen3.5-0.8b-general-forged (~500MB)"
      ;;
    mid)
      # 24-31GB: 2B general (~1.4GB GGUF). Bigger context window viable.
      PERSONA_MODEL="hf.co/continuum-ai/qwen3.5-2b-general-forged"
      info "Persona model tier: mid → qwen3.5-2b-general-forged (~1.4GB)"
      ;;
    mac_intel_discrete)
      # Mac Intel + discrete AMD / integrated Intel UHD. llama.cpp Metal
      # shaders broken on this path; smallest forged model + CPU-only.
      # Matches `tiers.mac_intel_discrete.default_chat` in
      # src/shared/models.json. When CambrianTech/llama.cpp lands the
      # Metal-AMD shader patch, this branch can promote to mid or full.
      PERSONA_MODEL="hf.co/continuum-ai/qwen3.5-0.8b-general-forged"
      info "Persona model tier: mac_intel_discrete → qwen3.5-0.8b-general-forged (~500MB, CPU-only)"
      ;;
    *)
      # 32GB+: original code-forged 4B (~2.7GB GGUF). Multimodal headroom.
      PERSONA_MODEL="hf.co/continuum-ai/qwen3.5-4b-code-forged-GGUF"
      ;;
  esac
  case "$IC_GPU_PATH" in
    dmr-*)
      # Per Joel 2026-05-04: "all the models must download and run on GPU"
      # + "we MUST have this work from ONE source of truth". DMR's
      # `docker model pull` was the Mac-only path that didn't work on
      # Linux. Models now download via the model-init container reading
      # src/shared/models.json — same path on Mac/Linux/Windows. The DMR
      # branch here remains for KV-cache-config + vLLM-MLX install (which
      # are still useful tuning), but no longer pulls the model.
      ok "Persona model download deferred to model-init container (reads src/shared/models.json)"
      # Cap llama-server's per-slot KV cache reservation, sized to actual
      # physical RAM. Without this cap each slot reserves the full model
      # context (262144 tokens for Qwen3.5), ballooning
      # com.docker.llama-server to 11+ GB resident on a single active slot
      # — observed live tonight on M5.
      #
      # Per-slot KV cost: ~16 KB per token at FP16 for Qwen3.5-4B
      # (32 layers × 256 attn-dim × 2 bytes × 2 tensors). Tier the cap so
      # 4 concurrent personas keep KV ≤10% of physical RAM:
      #
      #   Physical RAM   ctx-size   4-slot worst case
      #   8GB            4096       ~256 MB
      #   16GB           8192       ~512 MB
      #   24GB           16384      ~1 GB
      #   32GB           32768      ~2 GB
      #   48GB+          65536      ~4 GB
      #
      # Specialized recipes (codereview, research) can opt up via per-recipe
      # overrides — Phase 9 in docs/architecture/RESOURCE-ARCHITECTURE.md.
      if [[ -n "${PHYS_MIB:-}" ]]; then
        if   [[ "$PHYS_MIB" -ge $((48 * 1024)) ]]; then KV_CTX_SIZE=65536
        elif [[ "$PHYS_MIB" -ge $((32 * 1024)) ]]; then KV_CTX_SIZE=32768
        elif [[ "$PHYS_MIB" -ge $((24 * 1024)) ]]; then KV_CTX_SIZE=16384
        elif [[ "$PHYS_MIB" -ge $((16 * 1024)) ]]; then KV_CTX_SIZE=8192
        else                                            KV_CTX_SIZE=4096
        fi
      else
        # PHYS_MIB unset (shouldn't happen on Mac/Linux paths but be safe)
        KV_CTX_SIZE=8192
      fi
      if docker model configure show >/dev/null 2>&1; then
        if docker model configure --context-size "$KV_CTX_SIZE" --keep-alive 5m "$PERSONA_MODEL" 2>/dev/null; then
          ok "DMR context-size capped at ${KV_CTX_SIZE} + keep-alive 5m for $PERSONA_MODEL (sized to ${PHYS_GB:-?}GB physical RAM; kills the per-slot KV bloat)"
        else
          warn "Could not apply DMR context-size cap. Older Docker Desktop? Upgrade to 4.62+ for 'docker model configure'. Falling back to model default (high memory use)."
        fi
      else
        warn "'docker model configure' not available — Docker Desktop may be older than 4.62. Per-slot KV cache will use model default (~262k tokens, high RAM)."
      fi
      # Install vLLM MLX backend on Mac for 3x faster Qwen3.5 DeltaNet inference.
      # llama.cpp's Metal shaders for Gated DeltaNet are poorly optimized (~11 tok/s);
      # vllm-metal uses native MLX kernels (~33+ tok/s). Requires Docker Desktop 4.62+.
      if [[ "$OS" == "Darwin" ]]; then
        if docker model runner ls 2>/dev/null | grep -q "vllm"; then
          ok "vLLM MLX backend already installed"
        else
          info "Installing vLLM MLX backend for native Apple Silicon inference..."
          if docker model install-runner --backend vllm 2>/dev/null; then
            ok "vLLM MLX backend installed — Qwen3.5 DeltaNet will use native MLX kernels"
            # Pull MLX-format Qwen3.5-4B for vllm-metal routing.
            # DMR auto-routes MLX models to vllm-metal when installed.
            MLX_MODEL="hf.co/mlx-community/Qwen3.5-4B-MLX-4bit"
            # MLX-format model also moves to registry-driven download.
            # Add MLX entry to src/shared/models.json + auto_download.always
            # if/when we want vllm-metal to find it on disk.
            ok "MLX model download deferred to model-init (add to src/shared/models.json to enable)"
          else
            warn "vLLM install failed (requires Docker Desktop 4.62+). llama.cpp Metal will be used."
          fi
        fi
      fi
      ;;
    llama-vulkan)
      ok "Vulkan GPU path — model download handled by continuum-core at first inference"
      ;;
    unsupported)
      warn "No supported GPU detected. Local chat will error until a GPU adapter is available."
      ;;
  esac
fi

# ── Vision-capable model (Qwen2-VL-7B) — pull if missing ───────────
# The Vision AI persona uses the in-process llama.cpp adapter against
# Qwen2-VL-7B-Instruct + its multimodal projector (mmproj). Without
# both files on disk, AIProviderModule registers the adapter then logs
# the gap, and any image upload falls through to the text-bridge path
# (VisionDescriptionService) instead of going to a model that natively
# sees pixels — defeats the README's "see + speak" thesis.
#
# Total ~5.5 GB on disk (Q4_K_M GGUF + f16 mmproj). Pull with `hf
# download` (HuggingFace CLI; installed via `pip install huggingface-hub`
# which already happens earlier in install for the python deps). Skips
# cleanly if the files are already there.
#
# Path matches `models.toml::qwen2-vl-7b-instruct.gguf_local_path`
# (today: `~/models/qwen2-vl-7b/`). Loader expand_path resolves `~`.
QWEN2_VL_DIR="${HOME}/models/qwen2-vl-7b"
QWEN2_VL_GGUF="${QWEN2_VL_DIR}/Qwen2-VL-7B-Instruct-Q4_K_M.gguf"
QWEN2_VL_MMPROJ="${QWEN2_VL_DIR}/mmproj-Qwen2-VL-7B-Instruct-f16.gguf"
if [[ -f "$QWEN2_VL_GGUF" && -f "$QWEN2_VL_MMPROJ" ]]; then
  ok "Vision model already on disk: $QWEN2_VL_DIR"
else
  info "Pulling Vision AI model — Qwen2-VL-7B-Instruct (~5.5 GB, first install only)..."
  mkdir -p "$QWEN2_VL_DIR"
  if command -v hf >/dev/null 2>&1; then
    # `hf download` (huggingface-cli successor) — copies into local-dir
    # by default, no symlink dance. Both files in one call.
    if hf download bartowski/Qwen2-VL-7B-Instruct-GGUF \
        Qwen2-VL-7B-Instruct-Q4_K_M.gguf \
        mmproj-Qwen2-VL-7B-Instruct-f16.gguf \
        --local-dir "$QWEN2_VL_DIR" 2>/dev/null; then
      ok "Vision model pulled to $QWEN2_VL_DIR"
    else
      warn "Vision model pull failed. Manual: hf download bartowski/Qwen2-VL-7B-Instruct-GGUF Qwen2-VL-7B-Instruct-Q4_K_M.gguf mmproj-Qwen2-VL-7B-Instruct-f16.gguf --local-dir $QWEN2_VL_DIR"
      warn "Until pulled, the Vision AI persona will register but image uploads will hard-error."
    fi
  else
    warn "'hf' (huggingface-cli) not on PATH — can't auto-pull vision model."
    warn "Install: pip install huggingface-hub"
    warn "Then: hf download bartowski/Qwen2-VL-7B-Instruct-GGUF Qwen2-VL-7B-Instruct-Q4_K_M.gguf mmproj-Qwen2-VL-7B-Instruct-f16.gguf --local-dir $QWEN2_VL_DIR"
  fi
fi

# ── Audio-capable model (Qwen2-Audio-7B) — pull if missing ─────────
# Symmetric to the vision pull above. Audio AI persona uses the SAME
# in-process llama.cpp + libmtmd path the vision side uses
# (`backend.generate_with_audio()` → `MtmdContext::eval_audio()`),
# verified end-to-end 2026-04-22. Without both the GGUF + audio mmproj
# on disk, the adapter registers and any audio attachment falls through
# to the STT bridge — lossy: tone, pacing, non-speech sounds gone.
#
# mradermacher carries both files; bartowski / second-state / gaianet
# have weights only and are useless for libmtmd.
#
# Total ~5.7 GB on disk (Q4_K_M GGUF + f16 mmproj).
QWEN2_AUDIO_DIR="${HOME}/models/qwen2-audio-7b"
QWEN2_AUDIO_GGUF="${QWEN2_AUDIO_DIR}/Qwen2-Audio-7B-Instruct-Q4_K_M.gguf"
QWEN2_AUDIO_MMPROJ="${QWEN2_AUDIO_DIR}/mmproj-Qwen2-Audio-7B-Instruct-f16.gguf"
if [[ -f "$QWEN2_AUDIO_GGUF" && -f "$QWEN2_AUDIO_MMPROJ" ]]; then
  ok "Audio model already on disk: $QWEN2_AUDIO_DIR"
else
  info "Pulling Audio AI model — Qwen2-Audio-7B-Instruct (~5.7 GB, first install only)..."
  mkdir -p "$QWEN2_AUDIO_DIR"
  if command -v hf >/dev/null 2>&1; then
    # Note: mradermacher's repo names files with `.` separators (e.g.
    # `Qwen2-Audio-7B-Instruct.Q4_K_M.gguf`). Renamed locally to the
    # `-` convention models.toml expects so paths are consistent with
    # the vision sibling.
    if hf download mradermacher/Qwen2-Audio-7B-Instruct-GGUF \
        Qwen2-Audio-7B-Instruct.Q4_K_M.gguf \
        Qwen2-Audio-7B-Instruct.mmproj-f16.gguf \
        --local-dir "$QWEN2_AUDIO_DIR" 2>/dev/null && \
       mv "$QWEN2_AUDIO_DIR/Qwen2-Audio-7B-Instruct.Q4_K_M.gguf" "$QWEN2_AUDIO_GGUF" 2>/dev/null && \
       mv "$QWEN2_AUDIO_DIR/Qwen2-Audio-7B-Instruct.mmproj-f16.gguf" "$QWEN2_AUDIO_MMPROJ" 2>/dev/null; then
      ok "Audio model pulled to $QWEN2_AUDIO_DIR"
    else
      warn "Audio model pull failed. Manual: hf download mradermacher/Qwen2-Audio-7B-Instruct-GGUF Qwen2-Audio-7B-Instruct.Q4_K_M.gguf Qwen2-Audio-7B-Instruct.mmproj-f16.gguf --local-dir $QWEN2_AUDIO_DIR"
      warn "Until pulled, the Audio AI persona will register but audio uploads will fall back to STT bridge."
    fi
  else
    warn "'hf' (huggingface-cli) not on PATH — can't auto-pull audio model."
    warn "Install: pip install huggingface-hub"
    warn "Then: hf download mradermacher/Qwen2-Audio-7B-Instruct-GGUF Qwen2-Audio-7B-Instruct.Q4_K_M.gguf Qwen2-Audio-7B-Instruct.mmproj-f16.gguf --local-dir $QWEN2_AUDIO_DIR"
  fi
fi

# ── Per-service memory caps — auto-calculated from host RAM ────────
# Joel's directive: don't ask users to set mem limits; auto-calc from host.
# Don't paper over OOMs with undersized limits; size containers for the
# actual mission. Mission per-service budgets:
#
#   continuum-core (Linux container; on Mac it runs NATIVE and this cap
#   is informational / unused because docker-compose.mac.yml sets
#   replicas=0): needs to hold 4-8B param Qwen at Q4 (~4GB) + KV cache
#   for 5 concurrent personas (~2GB) + embeddings + Bevy + vision +
#   audio. Budget = host - 10GB (reserve for OS + Docker VM overhead
#   + support services).
#
#   livekit-bridge: native WebRTC encode/decode buffers, multiple
#   streams. Budget scales with host — roughly host/8.
#
#   node-server: TS orchestrator + IPC buffers + RAG state. Budget
#   same as livekit-bridge.
#
#   model-init: one-time downloader, fits in 2GB.
#
#   widget-server: static + light TS, 1GB.
#
#   postgres: our dataset is small, 512MB (already set in compose).
#   livekit server: 256m (already set in compose).
#
# Physical RAM is whichever host this runs on.
if [[ -n "${PHYS_MIB:-}" ]]; then
  # Mac branch set PHYS_MIB already. Linux sets it here from /proc/meminfo.
  :
elif [[ -f /proc/meminfo ]]; then
  PHYS_MIB=$(awk '/^MemTotal:/{print int($2/1024)}' /proc/meminfo)
fi

if [[ -n "${PHYS_MIB:-}" ]]; then
  PHYS_GB=$((PHYS_MIB / 1024))

  # continuum-core cap: (host - 10GB) on ≥32GB machines, (host - 8GB)
  # on 20-31GB machines. Floor at 10GB.
  if [[ $PHYS_GB -ge 32 ]]; then
    CONTINUUM_CORE_MEM=$((PHYS_GB - 10))g
  elif [[ $PHYS_GB -ge 20 ]]; then
    CONTINUUM_CORE_MEM=$((PHYS_GB - 8))g
  else
    CONTINUUM_CORE_MEM=10g
  fi

  # Scale livekit-bridge + node-server with host. Floor 2GB each.
  # 16GB host → 2g, 32GB → 4g, 64GB → 8g.
  SCALED=$((PHYS_GB / 8))
  [[ $SCALED -lt 2 ]] && SCALED=2
  LIVEKIT_BRIDGE_MEM=${SCALED}g
  NODE_SERVER_MEM=${SCALED}g

  # Static + small — these don't need to scale.
  MODEL_INIT_MEM=2g
  WIDGET_SERVER_MEM=1g

  export CONTINUUM_CORE_MEM LIVEKIT_BRIDGE_MEM NODE_SERVER_MEM MODEL_INIT_MEM WIDGET_SERVER_MEM

  info "Memory caps (${PHYS_GB}GB host): continuum-core=${CONTINUUM_CORE_MEM}, livekit-bridge=${LIVEKIT_BRIDGE_MEM}, node-server=${NODE_SERVER_MEM}, model-init=${MODEL_INIT_MEM}, widget-server=${WIDGET_SERVER_MEM}"
fi

# (OS-branch case/esac above handled Linux/Darwin and set CONTAINER_CMD.)
case "$OS" in
  Linux|Darwin) : ;;
  *) fail "Unsupported OS: $OS" ;;
esac

# ── 3. Clone / update repo ─────────────────────────────────
PHASE="clone / update repo"
# CONTINUUM_REF env override: clone a specific branch/sha instead of
# default (origin/HEAD). Used by carl-install-smoke CI to validate PR
# src/ changes — without it, install.sh always cloned origin/main and
# PR src/ edits never got tested by CI. 2026-05-03: this gap meant
# every fix to src/jtag, tools/scripts/install.sh, etc landed via PR
# but couldn't be validated by carl-install-smoke until merged. Joel:
# "months of trying to get continuum working out-of-box for Carl."
# Default ref is canary, NOT origin/HEAD (= main). main is intentionally
# behind canary until release cadence promotes the branch on schedule;
# 2026-05-03 main is 79 commits BEHIND canary, including critical install
# fixes (mod_jtag_bin_link, WSL2 config.env mirror, .env image-tag writer,
# resolveRoomIdentifier, stripLeakedToolMarkup, phantom-tab sanitize,
# socket chmod 666, etc). Default Carl install used to clone main and
# fail at line 769 with "mod_jtag_bin_link: command not found".
# Per Joel 2026-05-03: "Everyone uses current code period."
DEFAULT_CONTINUUM_REF="canary"
RESOLVED_CONTINUUM_REF="${CONTINUUM_REF:-$DEFAULT_CONTINUUM_REF}"

if [ -d "$INSTALL_DIR/.git" ]; then
  info "Updating existing installation..."
  cd "$INSTALL_DIR"
  git pull --ff-only 2>/dev/null || warn "Could not update — using existing version"
else
  info "Cloning Continuum at ref $RESOLVED_CONTINUUM_REF..."
  git clone --depth 1 --branch "$RESOLVED_CONTINUUM_REF" "$REPO" "$INSTALL_DIR" 2>/dev/null \
    || (git clone "$REPO" "$INSTALL_DIR" && cd "$INSTALL_DIR" && git checkout "$RESOLVED_CONTINUUM_REF")
  cd "$INSTALL_DIR"
fi

# ── 4. Shared modules (same code that Dev runs via npm start) ────
PHASE="shared modules"
# docs/infrastructure/INSTALL-ARCHITECTURE.md §Module-shape: the canonical
# module library at tools/scripts/lib/install-common.sh defines
# mod_submodules_init + mod_docker_wsl_integration + log/sudo primitives.
# Carl and Dev call the SAME functions so there's no drift.
if [ ! -f "tools/scripts/lib/install-common.sh" ]; then
  fail "Canonical install library missing at tools/scripts/lib/install-common.sh — incomplete clone? Try: rm -rf $INSTALL_DIR && re-run this installer."
fi

# shellcheck source=tools/scripts/lib/install-common.sh
source "tools/scripts/lib/install-common.sh"

mod_submodules_init
mod_docker_wsl_integration

# Real daemon check. On Linux this verifies Docker Engine is up (after the
# WSL integration module had a chance to fix it on Windows/WSL2 hosts);
# on Mac it verifies `podman machine start` above actually connected.
if ! $CONTAINER_CMD info &>/dev/null 2>&1; then
  case "$OS" in
    Darwin) fail "Podman machine not reachable. Run: podman machine start — then re-run this installer." ;;
    *)      fail "Docker daemon not reachable. Start Docker Desktop / Rancher Desktop and re-run." ;;
  esac
fi
ok "$CONTAINER_CMD $($CONTAINER_CMD version --format '{{.Client.Version}}' 2>/dev/null || echo 'ready')"
ok "Source: $INSTALL_DIR"

# ── 3a. Build host-side CLI bundle (BEST-EFFORT — old Node client layer) ──
# The jtag CLI bundle (dist/cli-bundle.js) is part of the OLD Node/TS client
# layer. That layer is being REINVENTED on the new client SDK, not repaired
# (screenshot-as-spec rebuild as its own modular container — see the
# client-SDK rework). With the headless-Rust work it's also prone to
# directory-reshuffle breakage (e.g. browser-index resolving to tools/
# instead of src/). So the host-side bundle build is now BEST-EFFORT and
# NON-BLOCKING: the install deliverable is the headless Rust core (brought
# up below via the container runtime), not this bundle.
#
# When the bundle builds, jtag gets its fast path. When it doesn't, install
# still SUCCEEDS — the core is up; jtag falls back to `tsx cli.ts`, and the
# new client SDK is the real path forward. We warn loudly (never silently
# claim success) but we never abort the whole install on this old layer.
PHASE="host-side jtag CLI bundle (best-effort)"
CLI_BUNDLE_OK=0
if [ ! -f "$INSTALL_DIR/src/package.json" ]; then
  warn "src/package.json missing — skipping host-side jtag CLI bundle (headless core install continues)."
elif ! command -v npm >/dev/null 2>&1; then
  warn "npm not on PATH — skipping host-side jtag CLI bundle (headless core install continues; install Node.js to get the jtag fast path)."
else
  info "Building host-side jtag CLI bundle (best-effort, ~30-60s)..."
  # build:cli takes dist/cli.js as INPUT (esbuild input file). dist/cli.js
  # is OUTPUT of build:ts. So the right invocation is `npm run build`
  # (which is build:ts → postbuild → build:cli per package.json scripts).
  if (
    set -e
    cd "$INSTALL_DIR/src"
    echo "  → npm install (~10s)..."
    npm install 2>&1 | tail -5
    echo "  → npm run build (TypeScript compile + esbuild bundle, ~30-50s)..."
    npm run build 2>&1 | tail -10
  ) && [ -f "$INSTALL_DIR/src/dist/cli-bundle.js" ]; then
    CLI_BUNDLE_OK=1
    ok "jtag CLI bundle ready ($INSTALL_DIR/src/dist/cli-bundle.js)"
  else
    warn "Host-side jtag CLI bundle did not build (old Node client layer, under rework)."
    warn "  Install CONTINUES — the headless core is the deliverable. jtag will fall"
    warn "  back to 'tsx cli.ts'. To retry manually: cd $INSTALL_DIR/src && npm install && npm run build"
  fi
fi

# ── 3b. Install continuum command (modular, headless-safe) ─
# Was an inline `sudo cp` that crashed on "no TTY for password" when the
# install ran headless (curl|bash without -t, BigMama SSH dry-run, CI).
# Now goes through mod_continuum_bin_link which routes to a user-space
# fallback (~/.local/bin) when sudo would prompt without a TTY.
mod_continuum_bin_link "$INSTALL_DIR/bin/continuum"

# Also place `jtag` on PATH — symlinked, not copied, so the launcher's
# BASH_SOURCE-based dist lookup keeps working. Without this, post-install
# `jtag <command>` (per CLAUDE.md / skill docs) returns command-not-found
# because src/jtag never gets a PATH entry. airc-8a5e 2026-05-03 Carl-UX
# QA caught this — chat-probe simulates `./jtag` from inside the install
# tree but real users follow the documented `jtag` form.
mod_jtag_bin_link "$INSTALL_DIR/src/jtag"

# ── 4. Configuration ───────────────────────────────────────
PHASE="configuration"
# Pre-create the directories the docker mount overlays. The continuum-core
# Dockerfile does `RUN mkdir -p /root/.continuum/sockets …` but the
# compose `~/.continuum:/root/.continuum` mount overlays that with the
# HOST's ~/.continuum at container start — so any subdir created at image
# build time becomes invisible inside the container. continuum-core then
# fails to bind its IPC socket with "IPC server error: No such file or
# directory (os error 2)" and the healthcheck never goes green, blocking
# the whole stack (continuum-core unhealthy → node-server's depends_on
# fails → compose up exits 1). Caught 2026-05-30 on carl-install-smoke
# of #1480; the canary image healthcheck regression had been silently
# blocking install-smoke for any install touching the docker stack.
mkdir -p "$CONTINUUM_DATA" "$CONTINUUM_DATA/sockets" \
         "$CONTINUUM_DATA/jtag/data" "$CONTINUUM_DATA/jtag/logs"

CONFIG_FILE="$CONTINUUM_DATA/config.env"
if [ ! -f "$CONFIG_FILE" ]; then
  info "Creating default config (zero API keys = local-only mode)..."
  cat > "$CONFIG_FILE" << 'EOF'
# Continuum Configuration — all API keys OPTIONAL
# System works with zero keys using local Candle inference.
# Add keys to enable cloud providers for better quality.

# ANTHROPIC_API_KEY=sk-ant-...
# OPENAI_API_KEY=sk-...
# DEEPSEEK_API_KEY=sk-...

HTTP_PORT=9000
WS_PORT=9001
EOF
  ok "Config: $CONFIG_FILE"
else
  ok "Config exists: $CONFIG_FILE"
fi

# WSL2 + Docker Desktop quirk: the bind mount `~/.continuum/config.env` in
# docker-compose.yml expands `~` on the Docker daemon side. On Windows the
# daemon runs as the Windows user so `~` resolves to C:\Users\<WinUser>,
# NOT the WSL user's /home/<linuxUser>. Without the file existing on the
# Windows-side path, Docker auto-vivifies an EMPTY DIRECTORY there — and
# then `compose up` fails with "mounting a directory onto a file" when it
# tries to mount that dir over /root/.continuum/config.env (a file path
# inside the container). Caught live by Carl-Windows install on
# bigmama-1 (continuum-b69f, 2026-05-03).
#
# Fix: on WSL2, mirror config.env to the Windows user's home so the file
# mount has a valid source. The OTHER bind mounts (`~/.continuum` dir)
# survive Docker's auto-vivify because dir-on-dir mount is fine, but the
# file mount needs the source to exist first.
#
# This is a no-op on Linux (no /mnt/c) and Mac (no /proc/version match).
if grep -qi microsoft /proc/version 2>/dev/null && [ -d /mnt/c ]; then
  WIN_USER="$(cmd.exe /c 'echo %USERNAME%' 2>/dev/null | tr -d '\r' | tr -d '\n')"
  if [ -n "$WIN_USER" ] && [ -d "/mnt/c/Users/$WIN_USER" ]; then
    WIN_CONTINUUM="/mnt/c/Users/$WIN_USER/.continuum"
    mkdir -p "$WIN_CONTINUUM"
    # If Docker auto-vivified an empty DIRECTORY where the file should
    # be, blow it away so we can write the file. rmdir refuses
    # non-empty dirs (so we don't clobber real user data); rm -rf only
    # if rmdir failed AND the dir is empty.
    if [ -d "$WIN_CONTINUUM/config.env" ]; then
      rmdir "$WIN_CONTINUUM/config.env" 2>/dev/null \
        || warn "Windows-side $WIN_CONTINUUM/config.env is a non-empty directory (likely user data); leaving it. May still hit the mount error — manually rm -rf and re-run if needed."
    fi
    if [ ! -e "$WIN_CONTINUUM/config.env" ]; then
      cp "$CONFIG_FILE" "$WIN_CONTINUUM/config.env"
      ok "Mirrored config.env to Windows path: $WIN_CONTINUUM/config.env"
    fi
  else
    warn "WSL2 detected but Windows username/home not found; config.env may not mount on Docker Desktop."
  fi
fi

# ── 5. TLS certs (Tailscale) ──────────────────────────────
PHASE="TLS certs (optional)"
TS_HOSTNAME=""
if command -v tailscale &>/dev/null; then
  TS_HOSTNAME=$(tailscale status --json 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin).get('Self',{}).get('DNSName','').rstrip('.'))" 2>/dev/null || echo "")

  if [ -n "$TS_HOSTNAME" ]; then
    if [ -f "$CONTINUUM_DATA/$TS_HOSTNAME.crt" ]; then
      ok "TLS: $TS_HOSTNAME (certs provisioned)"
    else
      info "Provisioning TLS certificate for $TS_HOSTNAME..."
      if tailscale cert "$TS_HOSTNAME" 2>/dev/null; then
        mv "$TS_HOSTNAME.crt" "$TS_HOSTNAME.key" "$CONTINUUM_DATA/"
        ok "TLS enabled: https://$TS_HOSTNAME"
      else
        warn "TLS cert failed — Tailscale Starter plan (\$6/month) required for HTTPS"
        warn "Enable at: https://login.tailscale.com/admin/dns → HTTPS Certificates"
      fi
    fi
  fi
else
  warn "Tailscale not installed — no mesh networking or TLS"
  warn "Optional: https://tailscale.com/download"
fi

# ── 6. Pick compose files + profile ───────────────────────
PHASE="compose files"
# Base file is always loaded. On GPU hosts, layer docker-compose.gpu.yml
# so continuum-core picks up the cuda image override (otherwise compose
# silently uses the CPU image and inference falls back to CPU). The same
# -f set MUST be passed to both `pull` and `up`, or pull grabs base
# images while up tries to use override-named images that aren't local.
COMPOSE_FILES="-f docker-compose.yml"
COMPOSE_ARGS=""
if [[ "$OS" == "Darwin" ]]; then
  # Mac path — the docker-compose.mac.yml override sets continuum-core's
  # replicas to 0 so support services boot in containers but continuum-core
  # stays off. We run continuum-core NATIVELY on Mac (via `npm start` below)
  # so Candle embeddings, Bevy headless render, vision processing, and audio
  # MPS paths all get real Metal. LLM inference routes to Docker Model
  # Runner's vllm-metal backend — also host-native, no container GPU tax.
  if [ -f "docker-compose.mac.yml" ]; then
    COMPOSE_FILES="$COMPOSE_FILES -f docker-compose.mac.yml"
  else
    warn "docker-compose.mac.yml missing — Mac detected but override won't apply. Without it, docker compose would try to run continuum-core in a container, which on Mac means CPU-only for Candle/Bevy/vision."
    fail "Fix: ensure you cloned with repository integrity — the Mac override file is part of the PR891 install architecture."
  fi
elif [[ "$HAS_GPU" == "true" ]]; then
  if [ -f "docker-compose.gpu.yml" ]; then
    COMPOSE_FILES="$COMPOSE_FILES -f docker-compose.gpu.yml"
  else
    warn "docker-compose.gpu.yml missing — GPU detected but cuda override won't apply. Continuing on Vulkan base image (still GPU-API; will use llvmpipe ICD if no vulkan driver)."
  fi
  COMPOSE_ARGS="--profile gpu"
fi
# Linux without a CUDA GPU: base docker-compose.yml uses continuum-core-vulkan.
# On real-driver hosts (Intel/AMD with vulkan) this picks up the hardware ICD;
# on hosts without a driver, mesa-vulkan-drivers (apt) provides llvmpipe as a
# software ICD so the Vulkan code path runs without panicking. Joel's
# 2026-04-23 rule: GPU integration is forbidden to fall back. Vulkan-via-
# llvmpipe is GPU integration (loader + ICD), not a CPU fallback.
if [[ "$OS" == "Linux" ]] && [[ "$HAS_GPU" != "true" ]]; then
  if ! command -v vulkaninfo >/dev/null 2>&1; then
    warn "vulkaninfo not found — install mesa-vulkan-drivers vulkan-tools so the Vulkan loader has the llvmpipe software ICD: sudo apt-get install -y mesa-vulkan-drivers vulkan-tools"
  elif ! vulkaninfo --summary 2>/dev/null | grep -qE "deviceName"; then
    warn "Vulkan loader present but enumerated zero devices. continuum-core-vulkan will panic on startup. Install: sudo apt-get install -y mesa-vulkan-drivers"
  else
    info "Vulkan loader OK — will use $(vulkaninfo --summary 2>/dev/null | grep -E 'deviceName' | head -1 | sed 's/.*= *//')"
  fi
fi

# ── 7. Pull support-service images ─────────────────────────
PHASE="pull images"
# Image tag resolution: compose files honor ${CONTINUUM_IMAGE_TAG:-latest}.
# Main-branch installs (Carl's default) use :latest. Reviewers validating
# a PR before merge can pin the PR's staged image set:
#   CONTINUUM_IMAGE_TAG=pr-891 curl -fsSL install.sh | bash
# CI tags every PR build with pr-<number> (see .github/workflows/docker-images.yml).
# Merging to main promotes that image set to :latest, so main and :latest
# are always in sync by construction.
#
# On Mac: `continuum-core` is not pulled (replicas=0 in docker-compose.mac.yml);
# only support services (postgres, node-server, widget-server, livekit-bridge,
# model-init) are pulled. continuum-core runs natively from `npm start` below.
# docker compose v2 substitution for ${CONTINUUM_IMAGE_TAG:-latest} reads
# from .env in the compose dir AND from shell env. In practice (observed
# 2026-05-03 on bigmama-1 + Carl-Windows install) it picks up .env
# reliably but NOT the shell env passed by install.sh — every compose
# invocation resolved to :latest even though install.sh exported the
# variable. Writing .env to $INSTALL_DIR (the compose-dir) before
# pulling images is the canonical fix per docs and works regardless of
# how the user invokes install.sh (curl|bash, direct, dispatched).
#
# Always write the .env (overwrite stale values from prior installs).
# CONTINUUM_IMAGE_TAG defaults to "latest" preserving the historical
# Carl path; explicit env override (e.g. CONTINUUM_IMAGE_TAG=canary
# curl|bash for testing canary) flows through unchanged.
EFFECTIVE_IMAGE_TAG="${CONTINUUM_IMAGE_TAG:-latest}"
{
  echo "# Auto-generated by install.sh — do not edit manually."
  echo "# Re-run install.sh to regenerate. Read by docker compose substitution."
  echo "CONTINUUM_IMAGE_TAG=$EFFECTIVE_IMAGE_TAG"
} > "$INSTALL_DIR/.env"

info "Pulling container images (tag: $EFFECTIVE_IMAGE_TAG)..."
$CONTAINER_CMD compose $COMPOSE_FILES $COMPOSE_ARGS pull 2>/dev/null || warn "Some images not published yet — will build locally"

# ── 8. Start support services ──────────────────────────────
PHASE="start support services"
# Inverse of parallel-start.sh's cross-mode detection: if native Dev-mode
# processes (continuum-core-server, tsx orchestrator) are running, docker
# compose up will collide on ports 9001/9100/7880-82/9003/5432. Warn so
# the user can stop them before starting the stack.
if pgrep -x 'continuum-core-server' >/dev/null 2>&1 \
   || pgrep -f 'tsx.*scripts/launch-active-example' >/dev/null 2>&1; then
  warn "Native Dev-mode continuum processes are running — they'll collide with the docker stack on ports."
  warn "Run 'cd src && npm stop' to stop BOTH native and any running docker stack (idempotent)."
  warn "Continuing — expect bind errors below if they persist."
fi
info "Starting support services..."
$CONTAINER_CMD compose $COMPOSE_FILES $COMPOSE_ARGS up -d


# Some published continuum-core images may predate the in-binary socket chmod
# fix (#1011). On Linux installs the host-side jtag CLI connects to the
# bind-mounted core socket — when the running image is older than #1011, the
# socket comes up root-owned without world-perms and host jtag gets EACCES.
# Workaround at install time until every architecture's heavy core image
# is refreshed past #1011.
fix_core_socket_permissions() {
  local socket_dir="$CONTINUUM_DATA/sockets"
  local core_socket="$socket_dir/continuum-core.sock"

  [ -d "$socket_dir" ] || return 1

  chmod 755 "$socket_dir" 2>/dev/null \
    || sudo -n chmod 755 "$socket_dir" 2>/dev/null \
    || warn "Could not chmod $socket_dir; host jtag may get EACCES"

  [ -S "$core_socket" ] || return 1

  chmod 666 "$core_socket" 2>/dev/null \
    || sudo -n chmod 666 "$core_socket" 2>/dev/null \
    || warn "Could not chmod $core_socket; host jtag may get EACCES"
}

if [[ "$OS" != "Darwin" ]]; then
  for _ in $(seq 1 60); do
    if fix_core_socket_permissions; then
      break
    fi
    sleep 1
  done
fi

# ── 8b. Start continuum-core natively on Mac ───────────────
# Mac runs continuum-core as a native host process so it can link Metal
# directly. `npm start` drives the full build (cargo build --release
# --features=metal + TS compile) and launches the server daemonized.
if [[ "$OS" == "Darwin" ]]; then
  info "Building + launching native continuum-core-server (Metal-enabled)..."
  info "  First run: cargo build takes 5-15 min. Subsequent runs: incremental."

  # No DATABASE_URL configured by default. Rust's data module defaults to
  # SQLite at ~/.continuum/database/main.db — zero-dep, portable, no
  # network topology gymnastics. For grid deployments (multi-writer over
  # Tailscale) users explicitly set DATABASE_URL in config.env AND run
  # `docker compose --profile postgres up`. All other callers (TS, tests,
  # jtag CLI) pass opaque handles; Rust resolves them to the configured
  # backend in modules/data.rs::resolve_handle.

  # CONTINUUM_CORE_TCP=9100 tells the native continuum-core-server to bind an
  # additional TCP listener alongside its Unix socket. Containerized
  # node-server (Option B Mac architecture) reaches the host-native
  # continuum-core via tcp://host.docker.internal:9100 because Unix sockets
  # don't traverse Docker Desktop's VM boundary on Mac. Native callers
  # (jtag CLI, continuum bin) keep using the Unix socket as before.
  #
  # CONTINUUM_CORE_BIND=0.0.0.0 is REQUIRED on Mac: Docker Desktop's
  # `host.docker.internal` resolves inside containers to the host's
  # docker-bridge IP (e.g. 192.168.65.254), NOT to 127.0.0.1. A loopback-
  # bound listener is unreachable from containers. 0.0.0.0 accepts on all
  # interfaces; macOS's application firewall blocks inbound LAN traffic
  # for unsigned dev binaries by default, so exposure stays local.
  export CONTINUUM_CORE_TCP=9100
  export CONTINUUM_CORE_BIND=0.0.0.0
  (cd "$INSTALL_DIR/src" && npm install --silent && npm start) || \
    warn "npm start failed — check logs at ~/.continuum/jtag/logs/system/continuum-core.log"
fi

# ── 8. Wait for widget-server health ───────────────────────
PHASE="widget-server health"
# Carl's experience hinges on this gate: if we open the browser before
# widget-server is actually serving, Chrome lands on the failed URL,
# replaces the location bar with chrome-error://chromewebdata/, and any
# subsequent reload tries to navigate from chrome-error back to http: —
# which the browser blocks as a cross-scheme navigation. Carl is then
# stuck on an error page with no clean recovery. Empirically: 2026-04-25
# joel hit "Unsafe attempt to load URL http://localhost:9003/ from frame
# with URL chrome-error://chromewebdata/" exactly because of this race.
#
# Two changes vs the prior 'curl -sf' wait:
#   1. Hit /health specifically (widget-server's health endpoint at
#      JTAGEndpoints.HEALTH = '/health'). A 200 here means widget-server
#      is actually serving HTTP, not just that the port is open.
#   2. If we never get a 200 in HEALTH_TIMEOUT_SEC, DO NOT open the
#      browser. Print actionable diagnostic + a manual-open command for
#      Carl to use after he checks the logs. Opening to a not-yet-ready
#      server is the bug; refusing to open is the correct behavior.
info "Waiting for widget-server health (timeout ${HEALTH_TIMEOUT_SEC:=120}s)..."
HEALTH_OK=0
for i in $(seq 1 "$HEALTH_TIMEOUT_SEC"); do
  # --fail returns non-zero on 4xx/5xx; --max-time keeps each probe snappy
  # so the loop stays close to a 1s cadence even when the server hangs.
  if curl -sf --max-time 2 http://localhost:9003/health >/dev/null 2>&1 \
     || curl -sfk --max-time 2 https://localhost:9003/health >/dev/null 2>&1; then
    HEALTH_OK=1
    ok "widget-server healthy after ${i}s"
    break
  fi
  sleep 1
done

# ── 8c. Wait for node-server seed to populate the default room ──────
# widget-server /health on port 9003 only proves that container is up.
# node-server (port 9001) runs auto-seed in docker-entrypoint.ts which
# creates the "general" room + personas. If the user opens the page or
# chat probe runs BEFORE seed completes, chat/send returns "Room not
# found: general" or "User not found" silently. Probe directly for the
# general room via jtag — fast, no new endpoint needed, deterministic.
# Caught by carl-install-smoke 2026-05-04 (PR #1038).
SEED_TIMEOUT_SEC="${SEED_TIMEOUT_SEC:-60}"
JTAG_BIN="$(command -v jtag 2>/dev/null || true)"
[ -z "$JTAG_BIN" ] && JTAG_BIN="$INSTALL_DIR/src/jtag"
if [ -x "$JTAG_BIN" ] && [ "$HEALTH_OK" -eq 1 ]; then
  info "Waiting for seed to populate default room (timeout ${SEED_TIMEOUT_SEC}s)..."
  SEED_OK=0
  for i in $(seq 1 "$SEED_TIMEOUT_SEC"); do
    # data/list returns success+items when the room exists. Empty items
    # means seed hasn't created it yet.
    if "$JTAG_BIN" data/list --collection=rooms --filter='{"uniqueId":"general"}' --limit=1 2>/dev/null \
       | grep -q '"success":true.*"items":\[{'; then
      SEED_OK=1
      ok "default room seeded after ${i}s"
      break
    fi
    sleep 1
  done
  if [ "$SEED_OK" -ne 1 ]; then
    warn "general room not present after ${SEED_TIMEOUT_SEC}s — seed may have failed."
    warn "  Chat will return 'Room not found' until seed completes."
    warn "  Diagnose: $CONTAINER_CMD compose -f $INSTALL_DIR/docker-compose.yml logs node-server | tail -50"
  fi
fi

# ── 9. Determine URL + open browser (only if healthy) ──────
PHASE="open browser"
if [ -n "$TS_HOSTNAME" ] && [ -f "$CONTINUUM_DATA/$TS_HOSTNAME.crt" ]; then
  URL="https://$TS_HOSTNAME:9003"
else
  URL="http://localhost:9003"
fi

if [ "$HEALTH_OK" -eq 1 ]; then
  case "$OS" in
    Darwin) open "$URL" 2>/dev/null || true ;;
    Linux)
      if grep -qi microsoft /proc/version 2>/dev/null; then
        cmd.exe /c start "" "$URL" 2>/dev/null || true
      else
        xdg-open "$URL" 2>/dev/null || true
      fi
      ;;
  esac
else
  warn "widget-server not healthy after ${HEALTH_TIMEOUT_SEC}s — NOT opening browser."
  warn "  Opening Chrome to a not-yet-ready URL traps you on a chrome-error page"
  warn "  that cannot cleanly recover. Diagnose + retry instead:"
  echo ""
  echo "    Logs:   $CONTAINER_CMD compose -f $INSTALL_DIR/docker-compose.yml logs --tail=200"
  echo "    Status: $CONTAINER_CMD compose -f $INSTALL_DIR/docker-compose.yml ps"
  echo "    Retry:  curl -v http://localhost:9003/health"
  echo ""
  echo "    Once the health endpoint returns 200, open the URL manually:"
  echo "      $URL"
  echo ""
fi

# ── Done ────────────────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Continuum is running"
echo ""
echo "  UI:      $URL"
echo ""
echo "  continuum          Open Continuum (from anywhere)"
echo "  continuum start    Start containers"
echo "  uu desktop         Open the desktop in your browser (uu = continuum; no port to remember)"
echo "  continuum stop     Stop containers"
echo "  continuum status   Show running state"
echo "  continuum open     Open browser"
echo ""
if [[ "$HAS_GPU" == "true" ]]; then
  echo "  GPU:     ${GPU_NAME:-detected}"
fi
if [ -n "$TS_HOSTNAME" ]; then
  echo "  Mesh:    https://$TS_HOSTNAME:9003"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
