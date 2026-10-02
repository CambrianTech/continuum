#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/../lib/cuda-targets.sh"
unset CUDA_PATH CUDA_COMPUTE_CAP
nvidia-smi() { printf '%s\n' "$caps"; }
nvcc() { printf 'compute_%s\n' 61 75 86 89 120; }
for pair in '6.1:61' '7.5:75' '8.6:86' '8.9:89' '12.0:120'; do
    caps="${pair%:*}"; unset CUDA_COMPUTE_CAP
    configure_cuda_targets
    [[ "$CMAKE_CUDA_ARCHITECTURES" == "${pair#*:}" && "$CUDA_COMPUTE_CAP" == "${pair#*:}" ]]
done
caps=$'12.0\n8.6\n6.1\n8.6'; unset CUDA_COMPUTE_CAP
configure_cuda_targets
[[ "$CMAKE_CUDA_ARCHITECTURES" == '61;86;120' && "$CUDA_COMPUTE_CAP" == 61 ]]
for caps in 'N/A' 6 6.10 99.0 ''; do
    unset CUDA_COMPUTE_CAP
    if configure_cuda_targets; then echo 'Accepted invalid/unsupported target' >&2; exit 1; fi
done
caps=6.1; export CUDA_COMPUTE_CAP=120
if configure_cuda_targets; then echo 'Accepted incompatible override' >&2; exit 1; fi
echo 'PASS CUDA families, mixed devices and refusal cases'
