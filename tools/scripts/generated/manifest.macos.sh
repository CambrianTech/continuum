# ==============================================================================
# GENERATED FILE - DO NOT EDIT.
# Rendered by `tools/manifest-gen` (cargo run -p manifest-gen) from `tools/scripts/install-manifest.toml`.
# The manifest is the ONE source of truth; this is a mechanical projection.
# Edit the manifest and regenerate. CI drift-check (`manifest-gen --check`)
# fails loud if this file is stale or hand-edited.
# ==============================================================================

# platform: macos
CONTINUUM_MODULES=('rust' 'gh' 'gh-auth' 'manifest-gen' 'cmake' 'llvm-libclang' 'build-core' 'run')

declare -A MOD_ORDER=( ['rust']='10' ['gh']='20' ['gh-auth']='25' ['manifest-gen']='28' ['cmake']='40' ['llvm-libclang']='50' ['build-core']='90' ['run']='100' )
declare -A MOD_TIER=( ['rust']='0' ['gh']='0' ['gh-auth']='0' ['manifest-gen']='3' ['cmake']='3' ['llvm-libclang']='3' ['build-core']='3' ['run']='3' )
declare -A MOD_FLAGS=( ['gh-auth']='grid' ['manifest-gen']='dev' ['cmake']='dev' ['llvm-libclang']='dev' ['build-core']='dev' )
declare -A MOD_APPLIES=()
declare -A MOD_ACCEPT=( ['rust']='rustc --version' ['gh']='gh --version' ['gh-auth']='gh auth status' ['manifest-gen']='cargo run -q -p manifest-gen -- --check' ['cmake']='cmake --version' ['llvm-libclang']='test -f /Library/Developer/CommandLineTools/usr/lib/libclang.dylib' ['build-core']='continuum-core-server.exe boots past the GPU-detection gate on the target device' ['run']='continuum-core-server binary present + serves TCP 9100' )
declare -A MOD_TYPE=( ['rust']='curl-sh' ['gh']='brew' ['manifest-gen']='command' ['cmake']='brew' ['llvm-libclang']='system' )
declare -A MOD_URL=( ['rust']='https://sh.rustup.rs' )
declare -A MOD_VERSION=()
declare -A MOD_SHA256=()
declare -A MOD_EXTRACT=()
declare -A MOD_REDIST_MANIFEST=()
declare -A MOD_COMPONENTS=()
declare -A MOD_FORMULA=( ['gh']='gh' ['cmake']='cmake' )
declare -A MOD_PACKAGE=()
declare -A MOD_ARGS=( ['rust']='-y --no-modify-path' )
declare -A MOD_RUN=( ['manifest-gen']='cargo run -q -p manifest-gen' )
declare -A MOD_BUILD_FEATURES=( ['build-core']='metal,accelerate' )
declare -A MOD_BUILD_PROFILE=( ['build-core']='release' )
declare -A MOD_RUNTIME_PATH=()
