#!/bin/bash
# Shared resolution + helpers for the MoE serving measurement harnesses in this directory.
#
# These are DEV HARNESSES, not the product path: the governed serving path is the Rust serving
# daemon, which picks residency division and cache budget through the governor. What lives here is
# the instrumentation that MEASURES the response surface the governor learns from, so the numbers
# stay reproducible by someone who is not the person who first ran them.
#
# Everything is resolved from the environment with a documented default and FAILS LOUD when it
# cannot be resolved. No silent fallback: a harness that quietly measures the wrong engine or the
# wrong model produces a number that looks real and is not.
#
# Environment:
#   CONTINUUM_LLAMA_ENGINE   path to the llama-server binary   (default: $HOME/.continuum/bin/llama-server-k3[.exe])
#   CONTINUUM_MODELS_DIR     directory holding the GGUF        (default: $HOME/.continuum/models)
#   CONTINUUM_MOE_MODEL_GLOB glob for the first shard          (default: */*00001-of-*.gguf, else *.gguf)
#   CONTINUUM_CUDA_DIR       CUDA toolkit root, if not on PATH (optional; needed on Windows for cublas)
#   CONTINUUM_WIN_BUILD_ENV  script that sets the MSVC/CUDA env (optional; Windows only)
#   CONTINUUM_MOE_PORT       port for the harness server       (default: 8130)
#   CONTINUUM_MOE_OUT_DIR    where logs and .jsonl land        (default: $CONTINUUM_MODELS_DIR/moe-runs)

set -uo pipefail

die() { echo "ERROR: $*" >&2; exit 1; }

# Windows only: pull in the MSVC + CUDA environment if the operator pointed at one.
if [ -n "${CONTINUUM_WIN_BUILD_ENV:-}" ]; then
    [ -f "$CONTINUUM_WIN_BUILD_ENV" ] || die "CONTINUUM_WIN_BUILD_ENV set but not a file: $CONTINUUM_WIN_BUILD_ENV"
    # shellcheck disable=SC1090
    source "$CONTINUUM_WIN_BUILD_ENV"
fi

if [ -n "${CONTINUUM_CUDA_DIR:-}" ]; then
    [ -d "$CONTINUUM_CUDA_DIR" ] || die "CONTINUUM_CUDA_DIR is not a directory: $CONTINUUM_CUDA_DIR"
    # Both layouts: the toolkit's bin/, and the conda-style Library/bin/ where cublas lands on Windows.
    export PATH="$CONTINUUM_CUDA_DIR/bin:$CONTINUUM_CUDA_DIR/Library/bin:$PATH"
fi

ENGINE="${CONTINUUM_LLAMA_ENGINE:-}"
if [ -z "$ENGINE" ]; then
    for cand in "$HOME/.continuum/bin/llama-server-k3.exe" "$HOME/.continuum/bin/llama-server-k3" \
                "$HOME/.continuum/bin/llama-server.exe"    "$HOME/.continuum/bin/llama-server"; do
        [ -x "$cand" ] && { ENGINE="$cand"; break; }
    done
fi
[ -n "$ENGINE" ] && [ -x "$ENGINE" ] || die "no llama-server engine found. Install it (tools/scripts/install-llama-server.sh) or set CONTINUUM_LLAMA_ENGINE."

MODELS_DIR="${CONTINUUM_MODELS_DIR:-$HOME/.continuum/models}"
[ -d "$MODELS_DIR" ] || die "models dir does not exist: $MODELS_DIR (set CONTINUUM_MODELS_DIR)"

OUT_DIR="${CONTINUUM_MOE_OUT_DIR:-$MODELS_DIR/moe-runs}"
mkdir -p "$OUT_DIR" || die "cannot create output dir: $OUT_DIR"

PORT="${CONTINUUM_MOE_PORT:-8130}"

# First shard of a sharded GGUF, else a single-file GGUF. Sharded models MUST be opened at shard 1.
resolve_model() {
    local glob="${CONTINUUM_MOE_MODEL_GLOB:-}"
    local m=""
    if [ -n "$glob" ]; then
        # shellcheck disable=SC2086
        m=$(ls $MODELS_DIR/$glob 2>/dev/null | head -1)
    else
        m=$(ls "$MODELS_DIR"/*/*00001-of-*.gguf "$MODELS_DIR"/*00001-of-*.gguf 2>/dev/null | head -1)
        [ -z "$m" ] && m=$(ls "$MODELS_DIR"/*/*.gguf "$MODELS_DIR"/*.gguf 2>/dev/null | head -1)
    fi
    [ -n "$m" ] || die "no GGUF found under $MODELS_DIR (set CONTINUUM_MOE_MODEL_GLOB)"
    echo "$m"
}

# Block until the server is listening, or a known-fatal line appears. Prints the outcome; never hangs
# forever silently — a harness that waits indefinitely reads as "still working" when it is dead.
# usage: wait_for_listen <logfile> [timeout_seconds]   -> 0 listening, 1 fatal, 2 timeout
wait_for_listen() {
    local log="$1" timeout="${2:-600}" waited=0
    while [ "$waited" -lt "$timeout" ]; do
        grep -qaE "listening on" "$log" 2>/dev/null && return 0
        grep -qaiE "out of memory|failed to allocate|error loading model|CUDA error|Segmentation|terminate called|unsupported" "$log" 2>/dev/null && return 1
        sleep 2; waited=$((waited + 2))
    done
    return 2
}

# Reap the engine BY NAME, not just by the shell's job pid. On Windows the shell's child is a wrapper
# and `kill $pid` leaves the real .exe alive holding VRAM — which silently corrupted every later point
# of a sweep with a card that was already full. Then wait for the VRAM to actually come back.
reap_engine() {
    local pid="${1:-}" name
    name=$(basename "$ENGINE")
    [ -n "$pid" ] && kill "$pid" 2>/dev/null
    if command -v taskkill >/dev/null 2>&1; then
        taskkill //F //IM "$name" >/dev/null 2>&1
    else
        pkill -f "$name" >/dev/null 2>&1
    fi
    if command -v nvidia-smi >/dev/null 2>&1; then
        for _ in $(seq 1 30); do
            local used
            used=$(nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits 2>/dev/null | head -1)
            [ "${used:-99999}" -lt 2000 ] && break
            sleep 2
        done
    else
        sleep 3
    fi
}

# Decode tok/s from a llama-server log: the LAST non-prompt "eval time" line. Empty if absent —
# callers must treat empty as "no measurement", never as zero.
decode_tok_s() {
    grep -aE "eval time =" "$1" 2>/dev/null | grep -v "prompt eval" | tail -1 \
        | sed -E 's/.*\(\s*[0-9.]+ ms per token,\s*([0-9.]+) tokens per second\)/\1/'
}

vram_used_mb() {
    nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits 2>/dev/null | head -1
}

CODING_PROMPT='{"messages":[{"role":"user","content":"Write a Python function to reverse a singly linked list. Code only."}],"max_tokens":96}'
