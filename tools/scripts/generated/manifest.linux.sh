# ==============================================================================
set -o pipefail  # a failing command in a pipeline must not read as success (card aad30dee)
# GENERATED FILE - DO NOT EDIT.
# Rendered by `tools/manifest-gen` (cargo run -p manifest-gen) from `tools/scripts/install-manifest.toml`.
# The manifest is the ONE source of truth; this is a mechanical projection.
# Edit the manifest and regenerate. CI drift-check (`manifest-gen --check`)
# fails loud if this file is stale or hand-edited.
# ==============================================================================

# platform: linux
CONTINUUM_MODULES=('rust' 'gh' 'gh-auth' 'manifest-gen' 'cmake' 'llvm-libclang' 'cuda' 'build-core' 'run')

declare -A MOD_ORDER=( ['rust']='10' ['gh']='20' ['gh-auth']='25' ['manifest-gen']='28' ['cmake']='40' ['llvm-libclang']='50' ['cuda']='60' ['build-core']='90' ['run']='100' )
declare -A MOD_TIER=( ['rust']='0' ['gh']='0' ['gh-auth']='0' ['manifest-gen']='3' ['cmake']='3' ['llvm-libclang']='3' ['cuda']='3' ['build-core']='3' ['run']='3' )
declare -A MOD_FLAGS=( ['gh-auth']='grid' ['manifest-gen']='dev' ['cmake']='dev' ['llvm-libclang']='dev' ['cuda']='dev' ['build-core']='dev' )
declare -A MOD_APPLIES=( ['cuda']='has-nvidia' )
declare -A MOD_ACCEPT=( ['rust']='rustc --version' ['gh']='gh --version' ['gh-auth']='gh auth status' ['manifest-gen']='cargo run -q -p manifest-gen -- --check' ['cmake']='cmake --version' ['llvm-libclang']='test -f /usr/lib/llvm-18/lib/libclang.so.1 || ldconfig -p | grep -q libclang' ['cuda']='nvcc --version >= 12.8' ['build-core']='continuum-core-server.exe boots past the GPU-detection gate on the target device' ['run']='continuum-core-server binary present + serves TCP 9100' )
declare -A MOD_TYPE=( ['rust']='curl-sh' ['gh']='apt' ['manifest-gen']='command' ['cmake']='apt' ['llvm-libclang']='apt' ['cuda']='redist' )
declare -A MOD_URL=( ['rust']='https://sh.rustup.rs' )
declare -A MOD_VERSION=( ['cuda']='12.9.1' )
declare -A MOD_SHA256=()
declare -A MOD_EXTRACT=()
declare -A MOD_REDIST_MANIFEST=( ['cuda']='https://developer.download.nvidia.com/compute/cuda/redist/redistrib_12.9.1.json' )
declare -A MOD_COMPONENTS=( ['cuda']='cuda_nvcc,cuda_cudart,libcublas,libcurand,cuda_nvrtc,cuda_cccl' )
declare -A MOD_FORMULA=()
declare -A MOD_PACKAGE=( ['gh']='gh' ['cmake']='cmake' ['llvm-libclang']='libclang-dev' )
declare -A MOD_ARGS=( ['rust']='-y --no-modify-path' )
declare -A MOD_RUN=( ['manifest-gen']='cargo run -q -p manifest-gen' )
declare -A MOD_BUILD_FEATURES=( ['build-core']='cuda,load-dynamic-ort' )
declare -A MOD_BUILD_PROFILE=( ['build-core']='release' )
declare -A MOD_RUNTIME_PATH=( ['cuda']='~/.continuum/cuda-*/lib:~/.continuum/cuda-*/lib64' )
