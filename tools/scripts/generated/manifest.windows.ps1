# ==============================================================================
# GENERATED FILE - DO NOT EDIT.
# Rendered by `tools/manifest-gen` (cargo run -p manifest-gen) from `tools/scripts/install-manifest.toml`.
# The manifest is the ONE source of truth; this is a mechanical projection.
# Edit the manifest and regenerate. CI drift-check (`manifest-gen --check`)
# fails loud if this file is stale or hand-edited.
# ==============================================================================

$script:ContinuumManifest = [ordered]@{
  'rust' = @{ order = 10; tier = 0; accept = 'rustc --version'; source = @{ type = 'winget'; id = 'Rustlang.Rustup'; scope = 'user' } }
  'gh' = @{ order = 20; tier = 0; accept = 'gh --version'; source = @{ type = 'winget'; id = 'GitHub.cli' } }
  'gh-auth' = @{ order = 25; tier = 0; flags = @('grid'); accept = 'gh auth status'; source = @{ type = 'command'; run = 'gh auth login --hostname github.com --git-protocol https --web' } }
  'airc-firewall' = @{ order = 27; tier = 0; flags = @('grid'); applies = 'has-airc'; accept = 'netsh advfirewall firewall show rule name="airc daemon inbound (continuum grid)"'; source = @{ type = 'command'; run = 'New-NetFirewallRule -DisplayName ''airc daemon inbound (continuum grid)'' -Direction Inbound -Action Allow -Profile Any' } }
  'manifest-gen' = @{ order = 28; tier = 3; flags = @('dev'); accept = 'cargo run -q -p manifest-gen -- --check'; source = @{ type = 'command'; run = 'cargo run -q -p manifest-gen' } }
  'msvc' = @{ order = 30; tier = 3; flags = @('dev'); accept = 'vswhere -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath'; source = @{ type = 'winget'; id = 'Microsoft.VisualStudio.2022.BuildTools'; override = '--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended' } }
  'cmake' = @{ order = 40; tier = 3; flags = @('dev'); accept = 'cmake --version'; source = @{ type = 'archive'; url = 'https://github.com/Kitware/CMake/releases/download/v4.4.2/cmake-4.4.2-windows-x86_64.zip'; version = '4.4.2'; sha256 = 'e8139d85b3813bc38833142ae1940472e9a587e9b5d2718ac1804c60f4e57a64'; extract = 'strip-top-dir' }; runtime_path = @('~/.continuum/tools/cmake/bin') }
  'llvm-libclang' = @{ order = 50; tier = 3; flags = @('dev'); accept = 'test-path ~/.continuum/tools/llvm/bin/libclang.dll'; source = @{ type = 'archive'; url = 'https://github.com/llvm/llvm-project/releases/download/llvmorg-18.1.8/clang+llvm-18.1.8-x86_64-pc-windows-msvc.tar.xz'; version = '18.1.8'; sha256 = '22c5907db053026cc2a8ff96d21c0f642a90d24d66c23c6d28ee7b1d572b82e8'; extract = 'members:*/bin/libclang.dll,*/lib/clang/*' }; runtime_path = @('~/.continuum/tools/llvm/bin') }
  'cuda' = @{ order = 60; tier = 3; flags = @('dev'); applies = 'has-nvidia'; accept = 'nvcc --version >= 12.8'; source = @{ type = 'redist'; version = '12.9.1'; manifest = 'https://developer.download.nvidia.com/compute/cuda/redist/redistrib_12.9.1.json'; components = @('cuda_nvcc', 'cuda_cudart', 'libcublas', 'libcurand', 'cuda_nvrtc', 'cuda_cccl') }; runtime_path = @('~/.continuum/cuda-*/Library/bin') }
  'build-core' = @{ order = 90; tier = 3; flags = @('dev'); accept = 'continuum-core-server.exe boots past the GPU-detection gate on the target device'; build = @{ features = 'cuda,load-dynamic-ort'; profile = 'release'; crt = 'static'; cmake_generator = 'Visual Studio 17 2022'; cuda_arch = '120'; msvc_host = 'vs2022' } }
  'run' = @{ order = 100; tier = 3; accept = 'continuum-core-server binary present + serves TCP 9100' }
}
