# win-modules.ps1 -- the Continuum native-build toolchain modules (Windows).
#
# Dot-sourced by install.ps1 AFTER install-common.ps1. Each Mod-* is a
# self-guarded, idempotent, auto-updating step following the same contract as
# the bash mod_* functions (guard -> applicability -> announce -> install -> done).
# Re-running is a no-op when satisfied and upgrades when below floor.
#
# Elevation policy (see install-common.ps1): per-user tools (rustup) install
# UN-elevated so they land in the invoking user's profile and the cargo build
# runs as that user; machine-scope tools (VS Build Tools, CMake, LLVM, CUDA, gh)
# go through the single gsudo credential cache (one UAC for all of them).

#  Manifest: the ONE source of truth (generated projection)
# Source values (urls, versions, sha256, redist components, build flags) live in
# install-manifest.toml and are projected to generated/manifest.windows.ps1 by
# manifest-gen. We SOURCE that projection here -- a value that lives in the
# manifest is NEVER hardcoded in a module. Regenerate with: cargo run -p manifest-gen
$script:ManifestPs = Join-Path $PSScriptRoot '..\generated\manifest.windows.ps1'
if (-not (Test-Path $script:ManifestPs)) {
    throw "manifest projection missing: $script:ManifestPs`n  regenerate it with: cargo run -p manifest-gen"
}
. $script:ManifestPs    # defines $script:ContinuumManifest ([ordered] hashtable)

# Fetch a module's projected record; fail loud if the manifest lacks it (a typo
# or a stale projection should stop the install, not silently skip a toolchain).
function Get-ManifestModule {
    param([Parameter(Mandatory = $true)][string]$Id)
    $m = $script:ContinuumManifest[$Id]
    if (-not $m) { throw "manifest has no module '$Id' -- check install-manifest.toml and regenerate (cargo run -p manifest-gen)" }
    return $m
}

# Verify a downloaded file against the manifest's sha256. archive sources carry a
# pinned hash; a mismatch means a corrupted download or a tampered/moved release
# -- fail loud, never install unverified bits. (redist components are verified by
# NVIDIA's own manifest, so they carry no per-file sha256 here.)
function Assert-Sha256 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Expected,
        [string]$Name = 'download'
    )
    $actual = (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $Expected.ToLowerInvariant()) {
        Module-Fail $Name "sha256 mismatch`n  expected $Expected`n  actual   $actual`n  ($Path -- corrupted download or moved release; do NOT install unverified)"
    }
}

#  GPU feature selection (PS port of tools/scripts/shared/cargo-features.sh)
# NVIDIA present -> cuda (candle-cuda + ggml-cuda, full native GPU). Otherwise
# directml (DX12 is universal on Win10+). --no-default-features is applied by
# Mod-BuildCore to drop livekit-webrtc (its /MT libwebrtc collides with the /MD
# rest -- the separate live-persona track).
function Get-CargoFeatures {
    # NVIDIA -> full GPU: the manifest's build-core features (candle+llama CUDA +
    # ORT CUDA EP; load-dynamic = ORT loaded at runtime, no build-time ORT lib).
    # Non-NVIDIA -> DirectML (runtime branch, not a manifest value: DX12 is
    # universal on Win10+, chosen by GPU presence not by the manifest).
    if (Get-Command nvidia-smi -ErrorAction SilentlyContinue) {
        return (Get-ManifestModule 'build-core').build.features
    }
    return 'directml'
}

#  CUDA version probe (Blackwell / sm_120 floor is 12.8) 
function Get-NvccVersion {
    if (-not (Get-Command nvcc -ErrorAction SilentlyContinue)) { return $null }
    $out = & nvcc --version 2>$null
    if ($out -match 'release\s+(\d+)\.(\d+)') {
        return [version]("{0}.{1}" -f $Matches[1], $Matches[2])
    }
    return $null
}

# No-admin, no-Python CUDA build toolkit, assembled from NVIDIA's OWN official
# redist archives (developer.download.nvidia.com/compute/cuda/redist). This is
# exactly what conda/pip repackage -- we skip the middleman: download NVIDIA's
# component .zips and merge them into one toolkit dir. No conda, no Python, no
# admin, no 3GB system installer. Blackwell (sm_120 / RTX 5090) needs >= 12.8.
$script:CudaToolkitDir = Join-Path $env:USERPROFILE '.continuum\cuda-toolkit'

function Get-CudaToolkitNvcc { return (Join-Path $script:CudaToolkitDir 'bin\nvcc.exe') }

function Test-CudaBuildToolkit {
    $nvcc = Get-CudaToolkitNvcc
    if (-not (Test-Path $nvcc)) { return $false }
    # Out-String: `& nvcc --version` yields a string ARRAY; `-match` on an array
    # filters and does NOT populate $Matches, so $Matches[1] would index null.
    $out = (& $nvcc --version 2>$null | Out-String)
    if ($out -match 'release (\d+)\.(\d+)') {
        return ([version]("{0}.{1}" -f $Matches[1], $Matches[2]) -ge [version]'12.8')
    }
    return $false
}

#  Modules 

function Mod-Rust {
    # Per-user: rustup installs to %USERPROFILE%\.cargo / .rustup so the build
    # (and later `npm start`) run as the user, not admin.
    $src = (Get-ManifestModule 'rust').source
    Install-IfMissing -Name 'Rust (rustup)' -WingetId $src.id `
        -TestCmd { Get-Command rustc -ErrorAction SilentlyContinue } -UserScope
    # The repo's rust-toolchain.toml pins the version and cargo auto-installs it
    # on first build; ensure a default toolchain exists so rustc/cargo resolve.
    if ((Get-Command rustup -ErrorAction SilentlyContinue) -and
        -not (Get-Command rustc -ErrorAction SilentlyContinue)) {
        & rustup default stable 2>&1 | Out-Null
        Update-SessionPath
    }
}

function Mod-VSBuildTools {
    # MSVC cl.exe / link.exe via the VS 2022 Build Tools C++ workload. The bare
    # package is only the installer shell -- the VCTools workload must be added
    # via --override. NOT --disable-interactivity (breaks this package,
    # winget-pkgs#123624); --wait + --quiet come through the override; exit 3010
    # (reboot) is handled as success by Install-IfMissing.
    $src = (Get-ManifestModule 'msvc').source
    Install-IfMissing -Name 'VS 2022 Build Tools (C++)' `
        -WingetId $src.id `
        -Override $src.override `
        -TestCmd { Test-VCTools }
}

function Mod-CMake {
    # Standalone Kitware CMake (knows every VS generator string, unlike the
    # VS-bundled one). Downloaded + extracted per-user -- NO admin.
    if (Get-Command cmake -ErrorAction SilentlyContinue) { Module-Skip 'CMake' 'on PATH'; return }
    $dir = Join-Path $env:USERPROFILE '.continuum\tools\cmake'
    $bin = Join-Path $dir 'bin'
    if (Test-Path (Join-Path $bin 'cmake.exe')) {
        if ($env:PATH -notlike "*$bin*") { $env:PATH = "$bin;$env:PATH" }
        Module-Skip 'CMake' "present at $dir"; return
    }
    Module-Start 'CMake' 'downloading Kitware CMake (no admin)'
    $src = (Get-ManifestModule 'cmake').source   # archive: url + version + sha256 + extract
    $ver = $src.version
    $url = $src.url
    $zip = Join-Path $env:TEMP "cmake-$ver.zip"
    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    Assert-Sha256 -Path $zip -Expected $src.sha256 -Name 'CMake'
    $tmp = Join-Path $env:TEMP 'continuum-cmake-x'; if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    $inner = Get-ChildItem $tmp -Directory | Select-Object -First 1
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item -Path (Join-Path $inner.FullName '*') -Destination $dir -Recurse -Force
    Remove-Item -Recurse -Force $tmp, $zip -ErrorAction SilentlyContinue
    if (Test-Path (Join-Path $bin 'cmake.exe')) { $env:PATH = "$bin;$env:PATH"; Module-Done 'CMake' }
    else { Module-Fail 'CMake' "cmake.exe not found after extract to $dir" }
}

function Mod-LLVM {
    # libclang.dll for bindgen. From LLVM's OFFICIAL release (clang+llvm
    # windows-msvc tarball), extracted per-user -- no admin, no Python.
    $dir = Join-Path $env:USERPROFILE '.continuum\tools\llvm'
    $bin = Join-Path $dir 'bin'
    if (Test-Path (Join-Path $bin 'libclang.dll')) {
        $env:LIBCLANG_PATH = $bin
        [Environment]::SetEnvironmentVariable('LIBCLANG_PATH', $bin, 'User')
        Module-Skip 'LLVM' "libclang present at $bin"; return
    }
    Module-Start 'LLVM' 'downloading libclang from LLVM official release (no admin)'
    # Version is PINNED in the manifest. The GitHub "latest" can be a bleeding-edge
    # RC whose libclang mis-generates llama.cpp's bindgen layout tests (llama_sampler
    # came out opaque[1 byte] vs the header's 16 -> a `1 - 16` E0080 underflow).
    # 18.1.x is the known-good that llama.cpp's bindgen expects. Bump in the
    # manifest (+ re-validate the llama build), never here.
    $src = (Get-ManifestModule 'llvm-libclang').source   # archive: url + version + sha256
    $url = $src.url
    $name = Split-Path $url -Leaf
    $tar = Join-Path $env:TEMP $name
    # Reuse a cached tarball (idempotent re-runs don't re-download ~800MB).
    if (-not ((Test-Path $tar) -and ((Get-Item $tar).Length -gt 100MB))) {
        Invoke-WebRequest -Uri $url -OutFile $tar -UseBasicParsing
    }
    Assert-Sha256 -Path $tar -Expected $src.sha256 -Name 'LLVM'
    New-Item -ItemType Directory -Force $dir | Out-Null
    # Use Windows' bsdtar EXPLICITLY -- git-bash's MSYS /usr/bin/tar reads the C:\
    # dest as a remote host ("cannot connect to C:") and fails. Extract only
    # bin/libclang.dll (fast); bindgen finds system headers via the MSVC env.
    $wtar = Join-Path $env:SystemRoot 'System32\tar.exe'
    & $wtar -xf $tar -C $dir --strip-components=1 "*/bin/libclang.dll" 2>$null
    if (-not (Test-Path (Join-Path $bin 'libclang.dll'))) {
        & $wtar -xf $tar -C $dir --strip-components=1 "*/bin/*" 2>$null
    }
    # Keep the tarball cached in TEMP for fast re-runs.
    if (Test-Path (Join-Path $bin 'libclang.dll')) {
        $env:LIBCLANG_PATH = $bin
        [Environment]::SetEnvironmentVariable('LIBCLANG_PATH', $bin, 'User')
        Module-Done 'LLVM'
    } else { Module-Fail 'LLVM' "libclang.dll not found after extract to $dir" }
}

function Mod-CUDA {
    # NVIDIA-only. Non-NVIDIA hosts build DirectML (no CUDA toolkit needed).
    if (-not (Get-Command nvidia-smi -ErrorAction SilentlyContinue)) {
        Module-Skip 'CUDA' 'no NVIDIA GPU -- native build will use DirectML'
        return
    }
    if (Test-CudaBuildToolkit) {
        $env:CUDA_PATH = $script:CudaToolkitDir
        Module-Skip 'CUDA' "toolkit present at $script:CudaToolkitDir"
        return
    }
    Module-Start 'CUDA' 'assembling no-admin CUDA toolkit from NVIDIA redist archives'

    # redist source (manifest url + version + components) from install-manifest.toml.
    # NVIDIA's redist manifest lists each component's windows-x86_64 archive path
    # (component versions differ), so we read the paths from it rather than
    # hardcode. The manifest URL already embeds the pinned redist version.
    $src = (Get-ManifestModule 'cuda').source
    $redistUrl = $src.manifest
    # Base dir the component relative_paths resolve against = URL up to the last '/'.
    # (Split-Path mangles URLs into backslashes; slice the string instead.)
    $base = $redistUrl.Substring(0, $redistUrl.LastIndexOf('/'))
    try {
        $manifest = Invoke-RestMethod -Uri $redistUrl -UseBasicParsing
    } catch {
        Module-Fail 'CUDA' "could not fetch NVIDIA redist manifest ($redistUrl) -- check network. ($_)"
    }

    # Component set (min to COMPILE ggml-cuda + candle-kernels: compiler, runtime,
    # cuBLAS, cuRAND (candle's RNG links curand.lib), NVRTC, CCCL headers) is DATA
    # in the manifest -- read it, don't hardcode.
    $components = $src.components
    New-Item -ItemType Directory -Force $script:CudaToolkitDir | Out-Null
    $tmp = Join-Path $env:TEMP 'continuum-cuda-redist'
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    New-Item -ItemType Directory -Force $tmp | Out-Null

    foreach ($c in $components) {
        $rel = $manifest.$c.'windows-x86_64'.relative_path
        if (-not $rel) { Write-Warn2 "CUDA: $c has no windows-x86_64 archive -- skipping"; continue }
        Write-Step "  $c"
        $zip = Join-Path $tmp (Split-Path $rel -Leaf)
        Invoke-WebRequest -Uri "$base/$rel" -OutFile $zip -UseBasicParsing
        $ext = Join-Path $tmp ("x_" + $c)
        Expand-Archive -Path $zip -DestinationPath $ext -Force
        # Each archive unpacks to <name>-archive/{bin,include,lib,nvvm,...};
        # merge those into the single unified toolkit dir.
        $inner = Get-ChildItem $ext -Directory | Select-Object -First 1
        if ($inner) {
            Copy-Item -Path (Join-Path $inner.FullName '*') -Destination $script:CudaToolkitDir -Recurse -Force
        }
    }
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue

    if (Test-CudaBuildToolkit) {
        $env:CUDA_PATH = $script:CudaToolkitDir
        Module-Done 'CUDA'
        Write-Ok "CUDA_PATH -> $script:CudaToolkitDir"
    } else {
        Module-Fail 'CUDA' "assembled toolkit but nvcc not runnable at $(Get-CudaToolkitNvcc)"
    }
}

function Mod-GhAuth {
    param([switch]$WantsGrid)
    # gh is needed for grid (gist rendezvous). Always ensure the CLI is present;
    # only prompt for login when grid is requested (mirrors the Tailscale opt-in).
    Install-IfMissing -Name 'GitHub CLI' -WingetId (Get-ManifestModule 'gh').source.id `
        -TestCmd { Get-Command gh -ErrorAction SilentlyContinue }
    if (-not $WantsGrid) { Module-Skip 'gh auth' 'local-only (no grid) -- GitHub login not required'; return }
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { Write-Warn2 'gh not on PATH yet -- re-run to finish login.'; return }
    & gh auth status 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { Module-Skip 'gh auth' 'already authenticated'; return }
    Module-Start 'gh auth' 'GitHub login for grid (gist rendezvous) -- device-code flow'
    & gh auth login --hostname github.com --git-protocol https --web
    if ($LASTEXITCODE -eq 0) { Module-Done 'gh auth' }
    else { Write-Warn2 'GitHub login not completed -- re-run install to finish, or: gh auth login' }
}

function Mod-BuildCore {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $core = Join-Path $RepoRoot 'core\continuum-core'
    if (-not (Test-Path $core)) {
        Module-Fail 'build' "continuum-core not found at $core -- run install.ps1 from a continuum repo checkout."
    }

    # Build cache: respect an existing CARGO_TARGET_DIR (an operator may point it
    # at a big drive), else default under the user profile. NEVER hardcode a drive
    # letter -- this is a public project, most machines have only C:.
    if (-not $env:CARGO_TARGET_DIR) {
        $env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE '.continuum\cache\cargo-target'
    }
    New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR | Out-Null

    # No-compromise GPU build (docs/architecture/GPU-CONTRACT.md): KEEP default
    # features (livekit + bevy stay on the GPU) and ADD the GPU features. NEVER
    # --no-default-features (that drops livekit off the GPU, a compromise). The
    # whole build is /MT via .cargo/config.toml's +crt-static, matching livekit's
    # prebuilt libwebrtc; cuda's nvcc host-compiles /MT the same way.
    $features = Get-CargoFeatures

    # GPU build env (NVIDIA path): nvcc needs its MSVC host compiler on PATH +
    # INCLUDE/LIB, and cmake/candle need to target the right VS + GPU arch. The
    # generator / arch / host are DATA in the manifest's build-core.build block.
    if (Get-Command nvidia-smi -ErrorAction SilentlyContinue) {
        $build = (Get-ManifestModule 'build-core').build
        Enter-MsvcEnv                                       # cl.exe for nvcc (nvcc-compatible VS)
        $env:CMAKE_GENERATOR = $build.cmake_generator       # match the VS nvcc supports
        $env:CMAKE_GENERATOR_PLATFORM = 'x64'
        $env:CMAKE_CUDA_ARCHITECTURES = $build.cuda_arch    # Blackwell RTX 5090 = sm_120
        if (-not $env:CUDA_COMPUTE_CAP) { $env:CUDA_COMPUTE_CAP = $build.cuda_arch }  # candle-kernels
        if ($env:CUDA_PATH) {
            $env:CUDA_HOME = $env:CUDA_PATH
            $cudaBin = Join-Path $env:CUDA_PATH 'bin'
            if ((Test-Path $cudaBin) -and ($env:PATH -notlike "*$cudaBin*")) { $env:PATH = "$cudaBin;$env:PATH" }
        }
    }

    Module-Start 'build' "cargo build continuum-core-server (release, default features + $features)"

    Push-Location $core
    try {
        # Runs as the invoking user (NOT via gsudo) so the cache stays user-owned.
        # Build BOTH the serving binary AND `cu` (the CLI the user + agents drive
        # the core with -- `cu ping`, `cu memory/*`, etc.). An install that ships
        # the server but not its CLI is only half a product.
        $buildArgs = @('build', '-p', 'continuum-core',
            '--bin', 'continuum-core-server', '--bin', 'cu',
            '--release', '--features', $features)
        & cargo @buildArgs
        $code = $LASTEXITCODE
    } finally { Pop-Location }

    if ($code -ne 0) {
        Module-Fail 'build' "cargo build failed ($code). The toolchain modules provision MSVC/CMake/LLVM/CUDA; check the output above."
    }
    Module-Done 'build'
}

function Mod-Run {
    $bin = Join-Path $env:CARGO_TARGET_DIR 'release\continuum-core-server.exe'
    if (Test-Path $bin) {
        Module-Done 'run'
        Write-Ok "continuum-core-server.exe ready: $bin"
        Write-Host '    Start the full system with:  npm start'
    } else {
        Write-Warn2 "serving binary not found at $bin -- the build step may not have completed."
    }
}
