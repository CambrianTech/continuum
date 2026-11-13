#!/usr/bin/env bash
# Same contract as cuda-targets.ps1; platform commands are adapters, not policy.
configure_cuda_targets() {
    local detected supported cap arch lowest='' targets='' compiler='nvcc'
    detected="$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader)" || { echo 'CUDA device capability query failed' >&2; return 1; }
    if [[ -n "${CUDA_PATH:-}" ]]; then
        # Both toolkit layouts windows-build-env.sh accepts: NVIDIA's (bin/) and conda's
        # (Library/bin/, the 5090's cuda-13.2). Only bin/ was tried, so every 5090 build
        # failed "CUDA compiler architecture query failed".
        compiler=''
        for candidate in "$CUDA_PATH"/{bin,Library/bin}/nvcc{,.exe}; do
            [[ -x "$candidate" && -f "$candidate" ]] && { compiler="$candidate"; break; }
        done
        [[ -n "$compiler" ]] || { echo "No nvcc under CUDA_PATH=$CUDA_PATH (bin/ or Library/bin/)" >&2; return 1; }
    fi
    supported="$("$compiler" --list-gpu-arch)" || { echo 'CUDA compiler architecture query failed' >&2; return 1; }
    while IFS= read -r cap; do
        cap="${cap//$'\r'/}"; cap="${cap//[[:space:]]/}"
        [[ "$cap" =~ ^([1-9][0-9]*)\.([0-9])$ ]] || { echo "Invalid CUDA device capability: $cap" >&2; return 1; }
        arch="${BASH_REMATCH[1]}${BASH_REMATCH[2]}"
        grep -qx "compute_$arch" <<< "${supported//$'\r'/}" || { echo "Selected CUDA toolkit cannot compile for detected compute_$arch; no CPU fallback" >&2; return 1; }
        targets+="$arch"$'\n'
    done <<< "$detected"
    targets="$(printf '%s' "$targets" | sort -nu)"
    lowest="${targets%%$'\n'*}"
    if [[ -n "${CUDA_COMPUTE_CAP:-}" ]]; then
        [[ "$CUDA_COMPUTE_CAP" =~ ^[1-9][0-9]+$ ]] && (( CUDA_COMPUTE_CAP <= lowest )) &&
            grep -qx "compute_$CUDA_COMPUTE_CAP" <<< "${supported//$'\r'/}" || {
                echo 'CUDA_COMPUTE_CAP override is incompatible with devices or toolkit' >&2; return 1;
            }
        lowest="$CUDA_COMPUTE_CAP"
    fi
    export CMAKE_CUDA_ARCHITECTURES="${targets//$'\n'/;}"
    export CUDA_COMPUTE_CAP="$lowest"
    printf 'CUDA targets: CMake=%s, Candle PTX=%s\n' "$CMAKE_CUDA_ARCHITECTURES" "$CUDA_COMPUTE_CAP" >&2
}

# A PUBLISHED build (CI's Windows core) runs on GPUs it never sees, so its Candle PTX targets a
# floor instead of a detected device: compute_80 (Ampere, RTX 30xx) JITs forward onto every newer
# card (the 5090's sm_120 included) and keeps candle's bf16 kernels, which compile only at >= 80
# (candle-kernels 0.9.2). Its fp8 kernels compile only at >= 89, so the published build has none.
# A node below the floor must not take the build; the manifest carries the value so the
# consumer can refuse it (prebuilt_artifact::manifest_verdict).
PUBLISHED_CUDA_COMPUTE_CAP=80
# The published ENGINE (llama-server) is different: built for 80 alone it carries machine code
# (SASS) for no consumer card, so on an RTX 30/40/50 the driver JIT-compiles every ggml-cuda kernel
# from the PTX the first time a binary loads (and caches it). On 2026-10-15 the 5090 rolled engine
# 56507cceb back after 240 s of HTTP 503 while engine 5b8500ee2, also built for 80, was ready in
# 23.45 s; a cold JIT cache for the new binary is the leading explanation, NOT proven (card
# 9b27d49b owes the cold-cache test). Either way a supported card should not depend on a JIT: the
# engine carries SASS for the consumer cards (Ampere 86: RTX 30xx; Ada 89: RTX 40xx; Blackwell
# 120: RTX 50xx) and 80 as SASS plus the PTX floor (A100, and the forward-JIT path for newer cards).
PUBLISHED_CUDA_ENGINE_ARCHITECTURES='80;86-real;89-real;120-real'
configure_published_cuda_targets() {
    local compiler='nvcc' supported entry
    if [[ -n "${CUDA_PATH:-}" ]]; then
        compiler=''
        for candidate in "$CUDA_PATH"/{bin,Library/bin}/nvcc{,.exe}; do
            [[ -x "$candidate" && -f "$candidate" ]] && { compiler="$candidate"; break; }
        done
        [[ -n "$compiler" ]] || { echo "No nvcc under CUDA_PATH=$CUDA_PATH (bin/ or Library/bin/)" >&2; return 1; }
    fi
    supported="$("$compiler" --list-gpu-arch)" || { echo 'CUDA compiler architecture query failed' >&2; return 1; }
    # every published target must be one the selected toolkit compiles (sm_120 needs CUDA 12.8+)
    for entry in ${PUBLISHED_CUDA_ENGINE_ARCHITECTURES//;/ } "$PUBLISHED_CUDA_COMPUTE_CAP"; do
        grep -qx "compute_${entry%%-*}" <<< "${supported//$'\r'/}" || {
            echo "Selected CUDA toolkit cannot compile published target ${entry%%-*}; refusing a published build without it" >&2; return 1;
        }
    done
    export CUDA_COMPUTE_CAP="$PUBLISHED_CUDA_COMPUTE_CAP"
    export CMAKE_CUDA_ARCHITECTURES="$PUBLISHED_CUDA_ENGINE_ARCHITECTURES"
    printf 'CUDA targets (published): engine CMake=%s, Candle PTX floor=%s\n' "$CMAKE_CUDA_ARCHITECTURES" "$CUDA_COMPUTE_CAP" >&2
}
