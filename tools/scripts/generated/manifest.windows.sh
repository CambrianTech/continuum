# ==============================================================================
# GENERATED FILE - DO NOT EDIT.
# Rendered by `tools/manifest-gen` (cargo run -p manifest-gen) from `tools/scripts/install-manifest.toml`.
# The manifest is the ONE source of truth; this is a mechanical projection.
# Edit the manifest and regenerate. CI drift-check (`manifest-gen --check`)
# fails loud if this file is stale or hand-edited.
# ==============================================================================

# platform: windows
CONTINUUM_MODULES=('rust' 'gh' 'gh-auth' 'airc-firewall' 'manifest-gen' 'msvc' 'cmake' 'llvm-libclang' 'cuda' 'build-core' 'run')

declare -A MOD_ORDER=( ['rust']='10' ['gh']='20' ['gh-auth']='25' ['airc-firewall']='27' ['manifest-gen']='28' ['msvc']='30' ['cmake']='40' ['llvm-libclang']='50' ['cuda']='60' ['build-core']='90' ['run']='100' )
declare -A MOD_TIER=( ['rust']='0' ['gh']='0' ['gh-auth']='0' ['airc-firewall']='0' ['manifest-gen']='3' ['msvc']='3' ['cmake']='3' ['llvm-libclang']='3' ['cuda']='3' ['build-core']='3' ['run']='3' )
declare -A MOD_FLAGS=( ['gh-auth']='grid' ['airc-firewall']='grid' ['manifest-gen']='dev' ['msvc']='dev' ['cmake']='dev' ['llvm-libclang']='dev' ['cuda']='dev' ['build-core']='dev' )
declare -A MOD_APPLIES=( ['airc-firewall']='has-airc' ['cuda']='has-nvidia' )
declare -A MOD_ACCEPT=( ['rust']='rustc --version' ['gh']='gh --version' ['gh-auth']='gh auth status' ['airc-firewall']='netsh advfirewall firewall show rule name="airc daemon inbound (continuum grid)"' ['manifest-gen']='cargo run -q -p manifest-gen -- --check' ['msvc']='vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath' ['cmake']='cmake --version' ['llvm-libclang']='test-path ~/.continuum/tools/llvm/bin/libclang.dll' ['cuda']='nvcc --version >= 12.8' ['build-core']='continuum-core-server.exe boots past the GPU-detection gate on the target device' ['run']='continuum-core-server binary present + serves TCP 9100' )
declare -A MOD_TYPE=( ['rust']='winget' ['gh']='winget' ['gh-auth']='command' ['airc-firewall']='command' ['manifest-gen']='command' ['msvc']='winget' ['cmake']='archive' ['llvm-libclang']='archive' ['cuda']='redist' )
declare -A MOD_URL=( ['cmake']='https://github.com/Kitware/CMake/releases/download/v3.30.5/cmake-3.30.5-windows-x86_64.zip' ['llvm-libclang']='https://github.com/llvm/llvm-project/releases/download/llvmorg-18.1.8/clang+llvm-18.1.8-x86_64-pc-windows-msvc.tar.xz' )
declare -A MOD_VERSION=( ['cmake']='3.30.5' ['llvm-libclang']='18.1.8' ['cuda']='12.9.1' )
declare -A MOD_SHA256=( ['cmake']='5ab6e1faf20256ee4f04886597e8b6c3b1bd1297b58a68a58511af013710004b' ['llvm-libclang']='22c5907db053026cc2a8ff96d21c0f642a90d24d66c23c6d28ee7b1d572b82e8' )
declare -A MOD_EXTRACT=( ['cmake']='strip-top-dir' ['llvm-libclang']='members:*/bin/libclang.dll,*/lib/clang/*' )
declare -A MOD_REDIST_MANIFEST=( ['cuda']='https://developer.download.nvidia.com/compute/cuda/redist/redistrib_12.9.1.json' )
declare -A MOD_COMPONENTS=( ['cuda']='cuda_nvcc,cuda_cudart,libcublas,libcurand,cuda_nvrtc,cuda_cccl' )
declare -A MOD_FORMULA=()
declare -A MOD_PACKAGE=()
declare -A MOD_ARGS=()
declare -A MOD_RUN=( ['gh-auth']='gh auth login --hostname github.com --git-protocol https --web' ['airc-firewall']='New-NetFirewallRule -DisplayName '\''airc daemon inbound (continuum grid)'\'' -Direction Inbound -Action Allow -Profile Any' ['manifest-gen']='cargo run -q -p manifest-gen' )
declare -A MOD_BUILD_FEATURES=( ['build-core']='cuda,load-dynamic-ort' )
declare -A MOD_BUILD_PROFILE=( ['build-core']='release' )
declare -A MOD_RUNTIME_PATH=( ['cmake']='~/.continuum/tools/cmake/bin' ['llvm-libclang']='~/.continuum/tools/llvm/bin' ['cuda']='~/.continuum/cuda-*/Library/bin' )
