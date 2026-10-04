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
