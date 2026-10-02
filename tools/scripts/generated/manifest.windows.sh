# ==============================================================================
# GENERATED FILE - DO NOT EDIT.
# Rendered by `tools/manifest-gen` (cargo run -p manifest-gen) from `tools/scripts/install-manifest.toml`.
# The manifest is the ONE source of truth; this is a mechanical projection.
# Edit the manifest and regenerate. CI drift-check (`manifest-gen --check`)
# fails loud if this file is stale or hand-edited.
# ==============================================================================

# platform: windows
CONTINUUM_MODULES=('gsudo' 'rust' 'gh' 'gh-auth' 'airc' 'airc-firewall' 'manifest-gen' 'msvc' 'cmake' 'xz-decoder' 'llvm-libclang' 'cuda' 'onnxruntime' 'poppler' 'build-core' 'run')

declare -A MOD_ORDER=( ['gsudo']='5' ['rust']='10' ['gh']='20' ['gh-auth']='25' ['airc']='26' ['airc-firewall']='27' ['manifest-gen']='28' ['msvc']='30' ['cmake']='40' ['xz-decoder']='49' ['llvm-libclang']='50' ['cuda']='60' ['onnxruntime']='70' ['poppler']='71' ['build-core']='90' ['run']='100' )
declare -A MOD_TIER=( ['gsudo']='0' ['rust']='0' ['gh']='0' ['gh-auth']='0' ['airc']='0' ['airc-firewall']='0' ['manifest-gen']='3' ['msvc']='3' ['cmake']='3' ['xz-decoder']='3' ['llvm-libclang']='3' ['cuda']='3' ['onnxruntime']='2' ['poppler']='2' ['build-core']='3' ['run']='3' )
declare -A MOD_FLAGS=( ['gh-auth']='grid' ['airc-firewall']='grid' ['manifest-gen']='dev' ['msvc']='dev' ['cmake']='dev' ['xz-decoder']='dev' ['llvm-libclang']='dev' ['cuda']='dev' ['build-core']='dev' )
declare -A MOD_APPLIES=( ['airc-firewall']='has-airc' ['cuda']='has-nvidia' ['onnxruntime']='has-nvidia' )
declare -A MOD_ACCEPT=( ['gsudo']='gsudo.exe --version' ['rust']='rustc --version' ['gh']='gh --version' ['gh-auth']='gh auth status' ['airc']='airc --help' ['airc-firewall']='AIRC public setup verifies effective TCP/UDP LocalSubnet policy for the installed executable' ['manifest-gen']='cargo run -q -p manifest-gen -- --check' ['msvc']='vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath' ['cmake']='cmake --version' ['xz-decoder']='xz --version' ['llvm-libclang']='test-path ~/.continuum/tools/llvm/bin/libclang.dll' ['cuda']='nvcc --version >= 12.8' ['onnxruntime']='test-path ~/.continuum/lib/onnxruntime.dll' ['poppler']='pdfinfo -v; pdftotext -v; pdftoppm -v' ['build-core']='continuum-core-server.exe boots past the GPU-detection gate on the target device' ['run']='continuum-core-server binary present + serves TCP 9100' )
declare -A MOD_TYPE=( ['gsudo']='winget' ['rust']='winget' ['gh']='winget' ['gh-auth']='command' ['airc']='curl-sh' ['airc-firewall']='command' ['manifest-gen']='command' ['msvc']='winget' ['cmake']='archive' ['xz-decoder']='archive' ['llvm-libclang']='archive' ['cuda']='redist' ['onnxruntime']='archive' ['poppler']='archive' )
declare -A MOD_URL=( ['airc']='https://raw.githubusercontent.com/CambrianTech/airc/canary/install.ps1' ['cmake']='https://github.com/Kitware/CMake/releases/download/v4.4.2/cmake-4.4.2-windows-x86_64.zip' ['xz-decoder']='https://github.com/tukaani-project/xz/releases/download/v5.8.4/xz-5.8.4-windows.zip' ['llvm-libclang']='https://github.com/llvm/llvm-project/releases/download/llvmorg-18.1.8/clang+llvm-18.1.8-x86_64-pc-windows-msvc.tar.xz' ['onnxruntime']='https://github.com/microsoft/onnxruntime/releases/download/v1.23.0/onnxruntime-win-x64-gpu-1.23.0.zip' ['poppler']='https://github.com/oschwartz10612/poppler-windows/releases/download/v26.09.0-0/Release-26.09.0-0.zip' )
declare -A MOD_VERSION=( ['cmake']='4.4.2' ['xz-decoder']='5.8.4' ['llvm-libclang']='18.1.8' ['cuda']='12.9.1' ['onnxruntime']='1.23.0' ['poppler']='26.09.0-0' )
declare -A MOD_SHA256=( ['cmake']='e8139d85b3813bc38833142ae1940472e9a587e9b5d2718ac1804c60f4e57a64' ['xz-decoder']='f31af7638391ecf286d48bc8555ce6e131691a1c52a9e44303bce57069e9de56' ['llvm-libclang']='22c5907db053026cc2a8ff96d21c0f642a90d24d66c23c6d28ee7b1d572b82e8' ['onnxruntime']='ec423825a8782c0bbd5cbfabcd984363fbb99ff084afae128719690d4db7f8d5' ['poppler']='7a6f256a0ddf7536182246a5733331bf4677cbcc34f4663774947ad34556c8d0' )
declare -A MOD_EXTRACT=( ['cmake']='strip-top-dir' ['xz-decoder']='preserve-tree' ['llvm-libclang']='members:*/bin/libclang.dll,*/lib/clang/*' ['onnxruntime']='strip-top-dir' ['poppler']='strip-top-dir' )
declare -A MOD_REDIST_MANIFEST=( ['cuda']='https://developer.download.nvidia.com/compute/cuda/redist/redistrib_12.9.1.json' )
declare -A MOD_COMPONENTS=( ['cuda']='cuda_nvcc,cuda_cudart,libcublas,libcurand,cuda_nvrtc,cuda_cccl' )
declare -A MOD_FORMULA=()
declare -A MOD_PACKAGE=()
declare -A MOD_ARGS=()
declare -A MOD_RUN=( ['gh-auth']='gh auth login --hostname github.com --git-protocol https --web' ['airc-firewall']='AIRC install.ps1 -FirewallOnly -AircPath <installed-executable>' ['manifest-gen']='cargo run -q -p manifest-gen' )
declare -A MOD_BUILD_FEATURES=( ['build-core']='cuda,load-dynamic-ort' )
declare -A MOD_BUILD_PROFILE=( ['build-core']='release' )
declare -A MOD_RUNTIME_PATH=( ['cmake']='~/.continuum/tools/cmake/bin' ['llvm-libclang']='~/.continuum/tools/llvm/bin' ['cuda']='~/.continuum/cuda-*/Library/bin:~/.continuum/cuda-toolkit/bin' ['poppler']='~/.continuum/tools/poppler/Library/bin' )
