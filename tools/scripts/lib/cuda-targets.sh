#!/usr/bin/env bash
# Same contract as cuda-targets.ps1; platform commands are adapters, not policy.
configure_cuda_targets() {
    local detected supported cap arch root candidate lowest='' targets='' compiler='nvcc'
    detected="$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader)" || { echo 'CUDA device capability query failed' >&2; return 1; }
    if [[ -n "${CUDA_PATH:-}" ]]; then
        root="$CUDA_PATH"
        if command -v cygpath >/dev/null 2>&1; then root="$(cygpath -u "$root")" || return 1; fi
        compiler=''
        for candidate in "$root/bin/nvcc" "$root/bin/nvcc.exe" "$root/Library/bin/nvcc" "$root/Library/bin/nvcc.exe"; do
            if [[ -x "$candidate" ]]; then compiler="$candidate"; break; fi
        done
        [[ -n "$compiler" ]] || { echo "Selected CUDA tree has no compiler: $CUDA_PATH" >&2; return 1; }
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
