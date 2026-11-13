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
# what this catches: a published engine without machine code for a consumer card makes that card
# JIT every kernel at its first launch (the 5090 rolled engine 56507cceb back after 240 s, card
# 9b27d49b); Candle's PTX floor stays 80 (its bf16 kernels need >= 80)
unset CUDA_COMPUTE_CAP CMAKE_CUDA_ARCHITECTURES
nvcc() { printf 'compute_%s\n' 80 86 89 100 120; }
configure_published_cuda_targets 2> /dev/null
[[ "$CUDA_COMPUTE_CAP" == 80 ]] || { echo "published Candle floor is $CUDA_COMPUTE_CAP, not 80" >&2; exit 1; }
for sass in 80 86 89 120; do
    [[ ";$CMAKE_CUDA_ARCHITECTURES;" =~ \;$sass(-real)?\; ]] || { echo "published engine lacks SASS for sm_$sass: $CMAKE_CUDA_ARCHITECTURES" >&2; exit 1; }
done
[[ ";$CMAKE_CUDA_ARCHITECTURES;" == *';80;'* ]] || { echo 'published engine lacks the compute_80 PTX floor (a bare 80 is SASS + PTX)' >&2; exit 1; }
# what this catches: a toolkit that can't compile a published target (sm_120 needs CUDA 12.8+) must
# refuse the published build, not ship an engine without the target it promises
nvcc() { printf 'compute_%s\n' 80 86 89; }
unset CUDA_COMPUTE_CAP CMAKE_CUDA_ARCHITECTURES
if configure_published_cuda_targets 2> /dev/null; then echo 'Accepted a toolkit without compute_120 for the published engine' >&2; exit 1; fi
echo 'PASS CUDA families, mixed devices, refusal cases, the published engine/candle split, and the toolkit refusal'
