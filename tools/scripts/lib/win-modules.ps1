. (Join-Path $PSScriptRoot 'payload-paths.ps1')
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
. (Join-Path $PSScriptRoot 'windows-engine-receipt.ps1')
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
    $out = Invoke-InstallerProcess 'nvcc' @('--version') 2>$null
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
function Get-CudaToolkitDirectory { return (Join-Path (Get-ManagedPayloadRoot) 'cuda-toolkit') }

function Get-CudaToolkitNvcc { return (Join-Path (Get-CudaToolkitDirectory) 'bin\nvcc.exe') }

function Test-CudaBuildToolkit {
    $nvcc = Get-CudaToolkitNvcc
    if (-not (Test-Path $nvcc)) { return $false }
    # Out-String: `& nvcc --version` yields a string ARRAY; `-match` on an array
    # filters and does NOT populate $Matches, so $Matches[1] would index null.
    $out = (Invoke-InstallerProcess $nvcc @('--version') 2>$null | Out-String)
    if ($out -match 'release (\d+)\.(\d+)') {
        return ([version]("{0}.{1}" -f $Matches[1], $Matches[2]) -ge [version]'12.8')
    }
    return $false
}

#  Cold storage: auto-detect a large drive and route cold artifacts there
# Machines like this (a workstation with a big spinning/secondary drive) should
# NOT pile multi-GB model GGUFs + the cargo build cache onto the system drive.
# DEFAULT behavior: find the roomiest non-system fixed drive and route cold
# storage there automatically -- models (HF cache + CONTINUUM_STORAGE_PATH) and
# the cargo build cache -- migrating what's already on the system drive. Generic:
# picks the roomiest drive by free space, NEVER a hardcoded letter (public
# project). The user can reconfigure later by editing ~/.continuum/config.env.

# Minimum free space for a drive to qualify as cold storage (below this it is not
# worth routing to). 256 GB is comfortably above a single model + build cache.
$script:ColdStorageMinFreeGB = 256

# Roomiest FIXED, non-system drive with >= min free. $null when there is none
# (single-drive laptop) -> we stay on the system drive.
function Get-ColdDrive {
    $sysQual = (Split-Path $env:USERPROFILE -Qualifier)   # e.g. 'C:'
    Get-Volume | Where-Object {
        $_.DriveLetter -and ($_.DriveType -eq 'Fixed') -and
        ("$($_.DriveLetter):" -ne $sysQual) -and
        ($_.SizeRemaining -ge ($script:ColdStorageMinFreeGB * 1GB))
    } | Sort-Object SizeRemaining -Descending | Select-Object -First 1
}

function Write-ColdMigrationRecord {
    param([string]$Path, [string]$Text)
    $temporary = $Path + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    try {
        [IO.File]::WriteAllText($temporary, $Text, (New-Object Text.UTF8Encoding($false)))
        # Same-directory rename publishes a complete record, refusing an owner
        # already at the destination. Interrupted writes cannot strand data.
        [IO.File]::Move($temporary, $Path)
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
}

function Assert-ColdMigrationPath {
    param([string]$Path)
    $cursor = [IO.Path]::GetFullPath($Path)
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $item = Get-Item -LiteralPath $cursor -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Cold migration path traverses a link/reparse point: $cursor"
            }
        }
        $parent = Split-Path $cursor -Parent
        if ($parent -eq $cursor) { break }
        $cursor = $parent
    }
}

# An owned receipt makes a partial move resumable. A destination without that
# receipt is never assumed to be this source's completed or interrupted copy.
function Move-ColdDir {
    param([string]$Src, [string]$Dst, [Parameter(Mandatory)][string]$ColdRoot)
    $Src = [IO.Path]::GetFullPath($Src).TrimEnd('\')
    $Dst = [IO.Path]::GetFullPath($Dst).TrimEnd('\')
    $cold = [IO.Path]::GetFullPath($ColdRoot).TrimEnd('\')
    $allowed = @{}
    foreach ($entry in @(@('.cache\huggingface','huggingface'), @('.continuum\genome','genome'), @('.continuum\cache\cargo-target','cargo-target'))) {
        $allowed[[IO.Path]::GetFullPath((Join-Path $env:USERPROFILE $entry[0]))] = $entry[1]
    }
    if (-not $allowed.ContainsKey($Src) -or $Dst -ine (Join-Path $cold $allowed[$Src]) -or
        $cold -ieq ([IO.Path]::GetPathRoot($cold)).TrimEnd('\') -or
        $Dst.StartsWith($Src + '\', [StringComparison]::OrdinalIgnoreCase) -or
        $Src.StartsWith($Dst + '\', [StringComparison]::OrdinalIgnoreCase) -or $Src -ieq $Dst) {
        throw 'Cold migration refused: source/destination are outside the selected cache roots.'
    }
    Assert-ColdMigrationPath $Src
    Assert-ColdMigrationPath $Dst
    $receipt = $Dst + '.continuum-migration'
    Assert-ColdMigrationPath $receipt
    $identity = "continuum-cold-migration-v1`n$Src`n$Dst`n"
    if (Test-Path -LiteralPath $receipt) {
        if ([IO.File]::ReadAllText($receipt) -ine $identity) { throw "Cold migration receipt names different paths: $receipt" }
    } elseif (-not (Test-Path -LiteralPath $Src)) { return }
    if (-not (Test-Path -LiteralPath $Src)) {
        if (-not (Test-Path -LiteralPath $Dst -PathType Container)) { throw 'Cold migration lost both endpoints; refusing to publish storage configuration.' }
        Remove-Item -LiteralPath $receipt -Force
        return
    }
    if (-not (Test-Path -LiteralPath $receipt)) {
        if (Test-Path -LiteralPath $Dst) { throw "Cold migration destination already exists without an ownership receipt: $Dst. No files were overwritten." }
        New-Item -ItemType Directory -Force $cold | Out-Null
        Write-ColdMigrationRecord -Path $receipt -Text $identity
    }
    Write-Step "  cold: migrating $Src -> $Dst"
    # Never traverse junctions; move symbolic links as links. Remaining source
    # entries cause refusal below, rather than silently publishing a partial cache.
    Invoke-InstallerProcess -OwnProcessTree 'robocopy' @($Src, $Dst, '/E', '/MOVE', '/SL', '/XJ', '/NFL', '/NDL', '/NP', '/R:1', '/W:1') | Out-Host
    $code = $global:LASTEXITCODE
    if ($code -ge 8) { throw "Cold migration failed with robocopy exit $code; the owned partial move will resume on installer rerun: $receipt" }
    if (Test-Path -LiteralPath $Src) {
        if (@(Get-ChildItem -LiteralPath $Src -Force).Count) { throw "Cold migration left source entries; refusing to publish partial storage: $Src" }
        Remove-Item -LiteralPath $Src -Force
    }
    if (-not (Test-Path -LiteralPath $Dst -PathType Container)) { throw 'Cold migration did not produce its destination.' }
    Remove-Item -LiteralPath $receipt -Force
    $global:LASTEXITCODE = 0
}

# Persist + export the cold-storage env so THIS install session (Mod-BuildCore)
# and every future process/core use the big drive. config.env carries
# CONTINUUM_STORAGE_PATH (the core reads it from there directly); HF_HOME +
# CARGO_TARGET_DIR are User env vars (hf-hub + cargo read them from the env).
function Set-ColdStorageEnv {
    param([Parameter(Mandatory = $true)][string]$ColdRoot)
    $hf = Join-Path $ColdRoot 'huggingface'
    $cargo = Join-Path $ColdRoot 'cargo-target'
    $configDir = Join-Path $env:USERPROFILE '.continuum'
    New-Item -ItemType Directory -Force $configDir | Out-Null
    $configEnv = Join-Path $configDir 'config.env'
    Update-ColdStorageConfig -Path $configEnv -ColdRoot $ColdRoot
    # Downloads/extraction must use the selected disk too, before LLVM/CUDA/ORT.
    # Session-only: do not redirect unrelated applications' temporary files.
    $temp = Join-Path $ColdRoot 'tmp'
    New-Item -ItemType Directory -Force $temp | Out-Null
    $env:TEMP = $temp
    $env:TMP = $temp
    foreach ($kv in @(@('CONTINUUM_STORAGE_PATH', $ColdRoot), @('HF_HOME', $hf), @('CARGO_TARGET_DIR', $cargo))) {
        [Environment]::SetEnvironmentVariable($kv[0], $kv[1], 'User')   # persist for future sessions
        Set-Item -Path "Env:$($kv[0])" -Value $kv[1]                    # and this session
    }
}

function Update-ColdStorageConfig {
    param([string]$Path, [string]$ColdRoot)
    # config.env is read both as dotenv and as shell source. Neither reader has
    # a shared escape syntax for embedded single quotes/newlines; fail before
    # modifying the file instead of persisting a different or executable value.
    if ($ColdRoot -match "['`r`n]") { throw 'Cold-storage path cannot contain single quotes or newlines in config.env.' }
    $text = if (Test-Path -LiteralPath $Path) { [IO.File]::ReadAllText($Path) } else { '' }
    $newline = if ($text.Contains("`r`n")) { "`r`n" } else { "`n" }
    foreach ($entry in @(@('CONTINUUM_STORAGE_PATH', $ColdRoot), @('HF_HOME', (Join-Path $ColdRoot 'huggingface')))) {
        $line = "$($entry[0])='$($entry[1])'"
        $pattern = '(?m)^[\t ]*' + $entry[0] + '[\t ]*=[^\r\n]*'
        if ([regex]::IsMatch($text, $pattern)) {
            # A delegate keeps dollar signs/backslashes in path values literal.
            $text = [regex]::Replace($text, $pattern, [System.Text.RegularExpressions.MatchEvaluator]{ param($match) $line })
        } else {
            if ($text -and -not $text.EndsWith("`n")) { $text += $newline }
            $text += $line + $newline
        }
    }
    $temporary = $Path + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    try {
        [IO.File]::WriteAllText($temporary, '')
        if (Test-Path -LiteralPath $Path) {
            # The file can contain credentials unrelated to storage; retain its
            # explicit access policy instead of inheriting broader parent ACLs.
            Set-Acl -LiteralPath $temporary -AclObject (Get-Acl -LiteralPath $Path)
        }
        [IO.File]::WriteAllText($temporary, $text, (New-Object Text.UTF8Encoding($false)))
        Move-Item -LiteralPath $temporary -Destination $Path -Force
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -Force }
    }
}

function Mod-ColdStorage {
    $configEnv = Join-Path $env:USERPROFILE '.continuum\config.env'
    $pending = Join-Path $env:USERPROFILE '.continuum\cold-storage.pending'
    $coldRoot = $null
    if (Test-Path -LiteralPath $pending) {
        $coldRoot = [IO.File]::ReadAllText($pending).TrimEnd("`r", "`n")
        if (-not (Test-Path -LiteralPath $coldRoot -PathType Container)) { throw 'Pending cold-storage drive is unavailable; setup stopped.' }
    }
    # Already routed to a still-present drive? Re-export env + skip (idempotent).
    if (-not $coldRoot -and (Test-Path $configEnv)) {
        $existing = Get-Content -LiteralPath $configEnv -Encoding UTF8 |
            Where-Object { $_ -match '^\s*CONTINUUM_STORAGE_PATH\s*=' } |
            ForEach-Object { ($_ -split '=', 2)[1].Trim() } | Select-Object -Last 1
        if ($existing -and $existing.Length -ge 2 -and
            (($existing.StartsWith("'") -and $existing.EndsWith("'")) -or
             ($existing.StartsWith('"') -and $existing.EndsWith('"')))) {
            $existing = $existing.Substring(1, $existing.Length - 2)
        }
        if ($existing -and (Test-Path (Split-Path $existing -Qualifier))) {
            Set-ColdStorageEnv -ColdRoot $existing
            Module-Skip 'cold-storage' "already routed to $existing (edit ~/.continuum/config.env to change)"
            return
        }
    }

    if (-not $coldRoot) {
        $cold = Get-ColdDrive
        if (-not $cold) {
            Module-Skip 'cold-storage' "no large secondary drive (>= $($script:ColdStorageMinFreeGB)GB free) -- staying on the system drive"
            return
        }
        $coldRoot = "$($cold.DriveLetter):\continuum-cold"
        New-Item -ItemType Directory -Force $coldRoot, (Split-Path $pending) | Out-Null
        Write-ColdMigrationRecord -Path $pending -Text ($coldRoot + "`n")
    }
    Module-Start 'cold-storage' "routing cold artifacts to $coldRoot"

    # Migrate what's already on the system drive (models cache, genome, build cache).
    Move-ColdDir (Join-Path $env:USERPROFILE '.cache\huggingface')            (Join-Path $coldRoot 'huggingface') -ColdRoot $coldRoot
    Move-ColdDir (Join-Path $env:USERPROFILE '.continuum\genome')             (Join-Path $coldRoot 'genome') -ColdRoot $coldRoot
    Move-ColdDir (Join-Path $env:USERPROFILE '.continuum\cache\cargo-target') (Join-Path $coldRoot 'cargo-target') -ColdRoot $coldRoot

    Set-ColdStorageEnv -ColdRoot $coldRoot
    Remove-Item -LiteralPath $pending -Force
    Module-Done 'cold-storage'
    Write-Ok "cold storage -> $coldRoot (models, genome, build cache). Reconfigure: ~/.continuum/config.env"
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
        Invoke-InstallerProcess -OwnProcessTree 'rustup' @('default', 'stable') 2>&1 | Out-Null
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

function Set-CMakeEnv {
    # Make our per-user cmake findable by EVERY future build shell, not just the
    # install session. The cmake-rs crate (llama's build.rs) honors the `CMAKE`
    # env var for the binary path -- exactly as bindgen honors LIBCLANG_PATH -- so
    # persisting CMAKE means a plain `cargo build` from a fresh terminal works, not
    # only `npm start` (which re-runs install and re-adds cmake to the session PATH
    # each time). PATH-safe: we set a named var, not mutate persistent PATH. Mirrors
    # Mod-LLVM's LIBCLANG_PATH persistence so the toolchain env is automatic.
    param([Parameter(Mandatory)][string]$Bin, [switch]$SessionOnly)
    $exe = Join-Path $Bin 'cmake.exe'
    $env:CMAKE = $exe
    if (-not $SessionOnly) { [Environment]::SetEnvironmentVariable('CMAKE', $exe, 'User') }
    if ($env:PATH -notlike "*$Bin*") { $env:PATH = "$Bin;$env:PATH" }  # also on PATH for direct CLI this session
}

function Get-CMakeVersion([string]$Exe) {
    # Parse `cmake --version` -> [version], or $null when unparsable. Out-String
    # because the native call yields a string ARRAY (same trap as the nvcc probe).
    $out = (Invoke-InstallerProcess $Exe @('--version') 2>$null | Out-String)
    if ($out -match 'cmake version (\d+)\.(\d+)\.(\d+)') {
        return [version]("{0}.{1}.{2}" -f $Matches[1], $Matches[2], $Matches[3])
    }
    return $null
}

function Mod-CMake {
    param([switch]$ExistingOnly)
    # Standalone Kitware CMake (knows every VS generator string, unlike the
    # VS-bundled one). Downloaded + extracted per-user -- NO admin.
    #
    # Skip requires VERSION SUFFICIENCY, not presence. This module used to skip
    # on 'cmake is on PATH' / 'cmake.exe exists at the install dir', so a box
    # carrying an older pin could NEVER upgrade by re-running the installer --
    # the re-run-is-update contract broken exactly where it mattered. Measured
    # on BigMama 2026-09-04: pinned 3.30.5 was present, VS 2026 was the box's
    # toolchain, 3.30.5 predates the 'Visual Studio 18 2026' generator (CMake
    # 4.2+), so every cargo build of the llama crate died at generator
    # selection while this module reported 'present'. Presence probes pass on
    # precisely the artifact that needs replacing.
    $src = (Get-ManifestModule 'cmake').source   # archive: url + version + sha256 + extract
    $ver = $src.version
    $pin = [version]$ver
    $onPath = Get-Command cmake -ErrorAction SilentlyContinue
    if ($onPath) {
        $have = Get-CMakeVersion $onPath.Source
        if ($have -and $have -ge $pin) { Module-Skip 'CMake' "on PATH ($have >= pinned $ver)"; return }
    }
    $dir = Join-Path (Get-ManagedPayloadRoot) 'tools\cmake'
    $bin = Join-Path $dir 'bin'
    $installedExe = Join-Path $bin 'cmake.exe'
    if (Test-Path $installedExe) {
        $have = Get-CMakeVersion $installedExe
        if ($have -and $have -ge $pin) {
            Set-CMakeEnv $bin -SessionOnly:$ExistingOnly
            Module-Skip 'CMake' "present at $dir ($have >= pinned $ver)"; return
        }
        if (-not $ExistingOnly) { Module-Start 'CMake' "upgrading $have -> $ver (older pin cannot know this box's VS generator)" }
    }
    else {
        if (-not $ExistingOnly) { Module-Start 'CMake' 'downloading Kitware CMake (no admin)' }
    }
    if ($ExistingOnly) { throw 'Preparation requires the pinned CMake already installed; run the normal installer to provision it.' }
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
    if (Test-Path (Join-Path $bin 'cmake.exe')) { Set-CMakeEnv $bin; Module-Done 'CMake' }
    else { Module-Fail 'CMake' "cmake.exe not found after extract to $dir" }
}

function Get-ManagedXzDecoder {
    # Windows bsdtar may delegate XZ to an external decoder. Acquire that
    # prerequisite from the manifest, in user-owned payload storage, before tar.
    $src = (Get-ManifestModule 'xz-decoder').source
    $dir = Join-Path (Get-ManagedPayloadRoot) 'tools\xz'
    $exe = Join-Path $dir 'bin_x86-64\xz.exe'
    if (Test-Path -LiteralPath $exe) {
        $version = (Invoke-InstallerProcess $exe @('--version') | Out-String)
        if ($LASTEXITCODE -eq 0 -and $version.Contains("XZ Utils) $($src.version)")) { return $exe }
    }
    Write-Host "  > [XZ] acquiring archive decoder $($src.version) (no admin)"
    $archive = Join-Path $env:TEMP "xz-$($src.version)-windows.zip"
    if (-not (Test-Path -LiteralPath $archive) -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $src.sha256) {
        $partial = $archive + '.download'
        Invoke-WebRequest -Uri $src.url -OutFile $partial -UseBasicParsing
        Assert-Sha256 -Path $partial -Expected $src.sha256 -Name 'XZ'
        Move-Item -LiteralPath $partial -Destination $archive -Force
    }
    Assert-Sha256 -Path $archive -Expected $src.sha256 -Name 'XZ'
    Expand-Archive -LiteralPath $archive -DestinationPath $dir -Force
    if (-not (Test-Path -LiteralPath $exe)) { throw 'XZ archive did not supply the required Windows decoder.' }
    $version = (Invoke-InstallerProcess $exe @('--version') | Out-String)
    if ($LASTEXITCODE -ne 0 -or -not $version.Contains("XZ Utils) $($src.version)")) { throw 'XZ decoder verification failed.' }
    return $exe
}

function Expand-ManagedTarXz {
    param([string]$Archive, [string]$Destination, [string[]]$Members)
    $decoder = Get-ManagedXzDecoder
    $savedPreference = $ErrorActionPreference
    $plainTar = Join-Path $Destination ([guid]::NewGuid().ToString('N') + '.tar')
    $process = New-Object Diagnostics.Process
    $process.StartInfo.FileName = $decoder
    $process.StartInfo.Arguments = '-d -c -- "' + $Archive + '"'
    $process.StartInfo.UseShellExecute = $false
    $process.StartInfo.CreateNoWindow = $true
    $process.StartInfo.RedirectStandardOutput = $true
    $process.StartInfo.RedirectStandardError = $true
    $started = $false
    try {
        # Windows bsdtar's external-filter pipes stalled on the real LLVM
        # archive after 64 KiB, although a tiny fixture passed. Decode to a
        # file first; never send binary tar bytes through PowerShell's pipeline.
        Write-Host '  > [archive] decoding XZ to temporary tar on the selected storage'
        $output = [IO.File]::Open($plainTar, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
        try {
            $started = $process.Start()
            if (-not $started) { throw 'Could not start XZ decoder.' }
            $errors = $process.StandardError.ReadToEndAsync()
            $process.StandardOutput.BaseStream.CopyTo($output)
            $process.WaitForExit()
            $detail = $errors.GetAwaiter().GetResult()
            if ($process.ExitCode -ne 0) { throw "Archive extraction failed during XZ decode (exit $($process.ExitCode)): $detail" }
        } finally { $output.Dispose() }
        Write-Host '  > [archive] extracting selected members from decoded tar'
        # Capture native stderr in PS5 without losing the exit status. A failed
        # decoder/extractor must never be reported as a completed prerequisite.
        $ErrorActionPreference = 'Continue'
        $diagnostic = @(Invoke-InstallerProcess (Join-Path $env:SystemRoot 'System32\tar.exe') (@('-xf', $plainTar, '-C', $Destination, '--strip-components=1') + $Members) 2>&1)
        $code = $LASTEXITCODE
    } finally {
        if ($started -and -not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        $process.Dispose()
        $ErrorActionPreference = $savedPreference
        if (Test-Path -LiteralPath $plainTar) { Remove-Item -LiteralPath $plainTar -Force }
    }
    if ($code -ne 0) { throw "Archive extraction failed (exit $code): $($diagnostic -join [Environment]::NewLine)" }
}

function Mod-LLVM {
    param([switch]$ExistingOnly)
    # libclang.dll for bindgen. From LLVM's OFFICIAL release (clang+llvm
    # windows-msvc tarball), extracted per-user -- no admin, no Python.
    $dir = Join-Path (Get-ManagedPayloadRoot) 'tools\llvm'
    $bin = Join-Path $dir 'bin'
    if (Test-Path (Join-Path $bin 'libclang.dll')) {
        $env:LIBCLANG_PATH = $bin
        if (-not $ExistingOnly) { [Environment]::SetEnvironmentVariable('LIBCLANG_PATH', $bin, 'User') }
        Module-Skip 'LLVM' "libclang present at $bin"; return
    }
    if ($ExistingOnly) { throw 'Preparation requires libclang already installed; run the normal installer to provision it.' }
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
    # Stage extraction before publishing; a decoder failure must not leave a
    # partial DLL that a rerun mistakes for an installed prerequisite.
    if (-not $src.extract.StartsWith('members:')) { throw 'LLVM manifest must declare archive members.' }
    $stageRoot = [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\')
    $stage = Join-Path $stageRoot ('continuum-llvm-' + [guid]::NewGuid().ToString('N'))
    if (-not ([IO.Path]::GetFullPath($stage)).StartsWith($stageRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe LLVM staging directory' }
    New-Item -ItemType Directory -Path $stage | Out-Null
    try {
        Expand-ManagedTarXz -Archive $tar -Destination $stage -Members $src.extract.Substring(8).Split(',')
        if (-not (Test-Path (Join-Path $stage 'bin\libclang.dll'))) { throw 'LLVM archive did not contain libclang.dll.' }
        Get-ChildItem -LiteralPath $stage | Copy-Item -Destination $dir -Recurse -Force -ErrorAction Stop
    } finally { Remove-Item -LiteralPath $stage -Recurse -Force }
    # Keep the tarball cached in TEMP for fast re-runs.
    if (Test-Path (Join-Path $bin 'libclang.dll')) {
        $env:LIBCLANG_PATH = $bin
        [Environment]::SetEnvironmentVariable('LIBCLANG_PATH', $bin, 'User')
        Module-Done 'LLVM'
    } else { Module-Fail 'LLVM' "libclang.dll not found after extract to $dir" }
}

function Mod-CUDA {
    param([switch]$ExistingOnly)
    # NVIDIA-only. Non-NVIDIA hosts build DirectML (no CUDA toolkit needed).
    if (-not (Get-Command nvidia-smi -ErrorAction SilentlyContinue)) {
        Module-Skip 'CUDA' 'no NVIDIA GPU -- native build will use DirectML'
        return
    }
    if (Test-CudaBuildToolkit) {
        $env:CUDA_PATH = (Get-CudaToolkitDirectory)
        Module-Skip 'CUDA' "toolkit present at $(Get-CudaToolkitDirectory)"
        return
    }
    if ($ExistingOnly) { throw 'Preparation requires the CUDA build toolkit already installed; run the normal installer to provision it.' }
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
    New-Item -ItemType Directory -Force (Get-CudaToolkitDirectory) | Out-Null
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
            Copy-Item -Path (Join-Path $inner.FullName '*') -Destination (Get-CudaToolkitDirectory) -Recurse -Force
        }
    }
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue

    if (Test-CudaBuildToolkit) {
        $env:CUDA_PATH = (Get-CudaToolkitDirectory)
        Module-Done 'CUDA'
        Write-Ok "CUDA_PATH -> $(Get-CudaToolkitDirectory)"
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
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'GitHub CLI is unavailable after provisioning; grid setup cannot continue.' }
    Invoke-InstallerProcess 'gh' @('auth', 'status') 2>$null | Out-Null
    if ($LASTEXITCODE -eq 0) { Module-Skip 'gh auth' 'already authenticated'; return }
    Module-Start 'gh auth' 'GitHub login for grid (gist rendezvous) -- device-code flow'
    Invoke-InstallerProcess 'gh' @('auth', 'login', '--hostname', 'github.com', '--git-protocol', 'https', '--web')
    if ($LASTEXITCODE -eq 0) { Module-Done 'gh auth' }
    else { throw "GitHub login did not complete (exit $LASTEXITCODE); grid setup stopped. Rerun this installer to resume authentication." }
}

function Invoke-AircSetup {
    param([string[]]$SetupArguments = @())
    $source = (Get-ManifestModule 'airc').source
    $scriptPath = Join-Path ([IO.Path]::GetTempPath()) ('continuum-airc-' + [guid]::NewGuid().ToString('N') + '.ps1')
    try {
        Invoke-WebRequest -Uri $source.url -OutFile $scriptPath -UseBasicParsing
        Invoke-InstallerProcess (Get-Process -Id $PID).Path (@('-NoProfile', '-ExecutionPolicy', 'RemoteSigned', '-File', $scriptPath) + $SetupArguments)
        if ($global:LASTEXITCODE -ne 0) { throw "AIRC setup failed (exit $global:LASTEXITCODE); the core was not restarted." }
    } finally { Remove-Item -LiteralPath $scriptPath -ErrorAction SilentlyContinue }
}

function Mod-AircFirewall {
    param([switch]$WantsGrid)
    # airc (the grid transport) listens for inbound PEER DIALS on an ephemeral TCP
    # port. Windows Firewall is ON by default on all profiles and SILENTLY DROPS
    # those inbound SYNs unless the airc daemon is allowed -- which manifests as a
    # baffling ASYMMETRIC route failure: outbound works (your messages reach peers),
    # but peers can't dial in, so cross-grid delivery + peer-dial (e.g. another node
    # routing inference to this box) never connect. A hand-added rule is a manual
    # step that a fresh grid box won't have -- so it belongs in the installer.
    #
    # AIRC owns its effective TCP/UDP local-subnet policy and legacy rule repair.
    # Its public entry borrows this installer's existing elevation owner.
    if (-not $WantsGrid) { Module-Skip 'airc-firewall' 'local-only (no grid) -- no inbound peer dials'; return }

    # Locate the airc grid-transport binary; skip cleanly if airc isn't installed.
    $airc = (Get-Command airc -ErrorAction SilentlyContinue).Source
    if (-not $airc) { $airc = Join-Path $env:USERPROFILE '.local\bin\airc.exe' }
    if (-not (Test-Path $airc)) { Module-Skip 'airc-firewall' 'airc not installed -- grid transport absent'; return }

    Module-Start 'airc-firewall' 'verifying AIRC-owned local-subnet TCP/UDP policy'
    Invoke-AircSetup -SetupArguments @('-FirewallOnly', '-AircPath', $airc)
    Module-Done 'airc-firewall'
}

function Mod-Airc {
    # Existing AIRC installations keep their selected channel and live daemon.
    # A fresh box uses AIRC's own supported installer, not a parallel bootstrap.
    $airc = Get-Command airc -ErrorAction SilentlyContinue
    $userBin = Join-Path $env:USERPROFILE '.local\bin'
    $canonicalBin = if ($env:BIN_TARGET) { $env:BIN_TARGET } else { Join-Path $env:USERPROFILE 'AppData\Local\Programs\airc' }
    $candidates = @((Join-Path $canonicalBin 'airc.exe'), (Join-Path $userBin 'airc.exe'))
    $installed = if ($airc) { $airc.Source } else { $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1 }
    if (-not $installed) {
        Invoke-AircSetup
        $installed = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if (-not $installed) { throw 'AIRC installer did not produce its configured CLI.' }
    }
    # A child installer cannot refresh this process's environment. Preserve the
    # toolchain additions already made here while exposing its actual bin dir.
    $aircDirectory = Split-Path $installed
    if (@($env:PATH -split ';' | Where-Object { $_.TrimEnd('\') -eq $aircDirectory }).Count -eq 0) { $env:PATH = $aircDirectory + ';' + $env:PATH }
}

function Mod-OrtRuntime {
    # DirectML builds obtain their runtime through ort at build time. CUDA's
    # load-dynamic-ort build needs this explicit runtime provision before launch.
    if (-not (Get-Command nvidia-smi -ErrorAction SilentlyContinue)) { return }
    if ($env:ORT_DYLIB_PATH) {
        if (-not (Test-Path -LiteralPath $env:ORT_DYLIB_PATH)) { throw 'Configured ORT_DYLIB_PATH does not exist.' }
        return
    }
    $lib = Join-Path (Get-ManagedPayloadRoot) 'lib'
    if (Test-Path (Join-Path $lib 'onnxruntime.dll')) { Module-Skip 'onnxruntime' 'installed runtime present'; return }
    $source = (Get-ManifestModule 'onnxruntime').source
    $scratch = Join-Path ([IO.Path]::GetTempPath()) ('continuum-ort-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $scratch | Out-Null
    try {
        $archive = Join-Path $scratch 'runtime.zip'
        Invoke-WebRequest -Uri $source.url -OutFile $archive -UseBasicParsing
        Assert-Sha256 -Path $archive -Expected $source.sha256 -Name 'onnxruntime'
        Expand-Archive -LiteralPath $archive -DestinationPath $scratch
        $library = @(Get-ChildItem -LiteralPath $scratch -Filter onnxruntime.dll -Recurse)
        if ($library.Count -ne 1) { throw 'ONNX Runtime archive has no unique runtime DLL.' }
        New-Item -ItemType Directory -Force -Path $lib | Out-Null
        Get-ChildItem -LiteralPath $library[0].DirectoryName -Filter '*.dll' |
            Copy-Item -Destination $lib -Force -ErrorAction Stop
        Module-Done 'onnxruntime'
    } finally {
        $resolved = [IO.Path]::GetFullPath($scratch)
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
        if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
            (Split-Path $resolved -Leaf) -notlike 'continuum-ort-*') { throw 'Unsafe runtime download cleanup path.' }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}

function Test-PopplerRuntime {
    param([string]$Directory, [string]$Version)
    foreach ($name in @('pdfinfo', 'pdftotext', 'pdftoppm')) {
        $exe = Join-Path $Directory ('Library\bin\' + $name + '.exe')
        if (-not (Test-Path -LiteralPath $exe)) { return $false }
        $start = New-Object Diagnostics.ProcessStartInfo
        $start.FileName = $exe
        $start.Arguments = '-v'
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardError = $true
        $process = New-Object Diagnostics.Process
        $process.StartInfo = $start
        try {
            if (-not $process.Start()) { return $false }
            if (-not $process.WaitForExit(5000)) { $process.Kill(); return $false }
            $reported = $process.StandardError.ReadToEnd()
            if ($process.ExitCode -ne 0 -or $reported -notmatch ([regex]::Escape($name + ' version ' + $Version) + '(\s|$)')) { return $false }
        } catch {
            # A present but corrupt/unloadable executable is drift, not a reason
            # to abort before the installer can replace the damaged bundle.
            return $false
        } finally { $process.Dispose() }
    }
    return $true
}

function Mod-Poppler {
    $source = (Get-ManifestModule 'poppler').source
    $version = ($source.version -split '-')[0]
    $directory = Join-Path (Get-ManagedPayloadRoot) 'tools\poppler'
    if (Test-PopplerRuntime -Directory $directory -Version $version) {
        Module-Skip 'poppler' 'all three installed PDF decoders execute at the pinned version'
        return
    }
    $scratch = Join-Path ([IO.Path]::GetTempPath()) ('continuum-poppler-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $scratch | Out-Null
    try {
        Module-Start 'poppler' 'installing checksum-pinned PDF runtime (no admin)'
        $archive = Join-Path $scratch 'runtime.zip'
        Invoke-WebRequest -Uri $source.url -OutFile $archive -UseBasicParsing
        Assert-Sha256 -Path $archive -Expected $source.sha256 -Name 'poppler'
        Expand-Archive -LiteralPath $archive -DestinationPath $scratch
        $roots = @(Get-ChildItem -LiteralPath $scratch -Directory)
        if ($roots.Count -ne 1 -or -not (Test-PopplerRuntime -Directory $roots[0].FullName -Version $version)) {
            throw 'Downloaded Poppler bundle does not execute all three PDF decoders at the pinned version.'
        }
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
        Get-ChildItem -LiteralPath $roots[0].FullName | Copy-Item -Destination $directory -Recurse -Force -ErrorAction Stop
        if (-not (Test-PopplerRuntime -Directory $directory -Version $version)) { throw 'Installed Poppler runtime validation failed.' }
        Module-Done 'poppler'
    } finally {
        $resolved = [IO.Path]::GetFullPath($scratch)
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
        if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
            (Split-Path $resolved -Leaf) -notlike 'continuum-poppler-*') { throw 'Unsafe PDF runtime cleanup path.' }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
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
    Protect-CoreBuildOutput -TargetDirectory $env:CARGO_TARGET_DIR

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
        # Build BOTH the serving binary AND `continuum` (the CLI the user + agents
        # drive the core with -- `continuum ping`, `continuum memory/*`, etc.). An
        # install that ships the server but not its CLI is only half a product.
        # (The CLI bin was `cu`; renamed to `continuum` in #2010 to kill the Unix
        # UUCP `cu` collision -- keep this arg in lockstep with the [[bin]] name.)
        $buildArgs = @('build', '-p', 'continuum-core',
            '--bin', 'continuum-core-server',
            '--release', '--features', $features)
        Invoke-InstallerProcess -FilePath 'cargo' -ArgumentList $buildArgs -OwnProcessTree
        $code = $LASTEXITCODE
        if ($code -eq 0) {
            # The client does not serve models or render frames. Keep its launch
            # independent of GPU DLLs, including while repairing the service.
            Invoke-InstallerProcess -FilePath 'cargo' -ArgumentList @('build', '-p', 'continuum-core', '--bin', 'continuum', '--release', '--no-default-features') -OwnProcessTree
            $code = $LASTEXITCODE
        }
        if ($code -eq 0) {
            Invoke-InstallerProcess -FilePath 'cargo' -ArgumentList @('build', '-p', 'livekit-bridge', '--bin', 'livekit-bridge', '--release') -OwnProcessTree
            $code = $LASTEXITCODE
        }
    } finally { Pop-Location }

    if ($code -ne 0) {
        Module-Fail 'build' "cargo build failed ($code). The toolchain modules provision MSVC/CMake/LLVM/CUDA; check the output above."
    }
    Module-Done 'build'
}

function Get-CoreEngineBackend {
    if (Get-Command nvidia-smi -ErrorAction SilentlyContinue) { return 'cuda' }
    return 'cpu'
}

function Get-CoreEngineRequirement {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $revision = (Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $RepoRoot, 'rev-parse', 'HEAD:core/vendor/llama.cpp') 2>$null)
    if ($LASTEXITCODE -ne 0 -or $revision -cnotmatch '^[0-9a-f]{40}$') { throw 'Cannot resolve the tracked llama.cpp gitlink.' }
    return [pscustomobject]@{ source_revision = $revision; backend = (Get-CoreEngineBackend) }
}

function Get-CoreEngineDrift {
    param([string]$Directory, [Parameter(Mandatory = $true)]$Requirement)
    try {
        $receipt = Get-CoreEngineReceipt -Directory $Directory
        if ($receipt.source_revision -cne $Requirement.source_revision -or $receipt.backend -cne $Requirement.backend) {
            return 'Installed engine receipt differs from the tracked source/backend.'
        }
        return ''
    } catch { return "Installed engine needs receipt convergence: $_" }
}

function Mod-LlamaServer {
    # Build the inference engine WE OWN: llama.cpp's `llama-server` (the OpenAI /v1
    # gateway) that continuum's serving daemon spawns as its GPU-backend CHILD
    # process. External-child is deliberate (M5): a CUDA-OOM-wedged backend can be
    # reaped + respawned without taking the core down. Windows twin of
    # tools/scripts/install-llama-server.sh (which covers macOS/Linux only). ONE
    # llama.cpp source of truth (core/vendor/llama.cpp) for both the in-process FFI
    # lib and this server child. Idempotent via a HEAD:backend stamp.
    #
    # The daemon probes $USERPROFILE/.continuum/bin/llama-server.exe (server_bin(),
    # airc/continuum serving fix 4d4c463fb) -> the BINARY lands there (system drive,
    # tiny). The heavy CUDA build tree goes to cold storage so it doesn't bloat C:.
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [string]$InstallDirectory = (Join-Path (Get-ManagedPayloadRoot) 'bin'),
        [switch]$RequireReceipt
    )

    $submodule   = Join-Path $RepoRoot 'core\vendor\llama.cpp'
    $serverCMake = Join-Path $submodule 'tools\server\CMakeLists.txt'
    $installDir  = $InstallDirectory
    $installBin  = Join-Path $installDir 'llama-server.exe'
    $stampFile   = Join-Path $installDir '.llama-server.stamp'
    # Build tree on the cold drive when cold-storage routed one (else system cache).
    $cacheRoot   = if ($env:CONTINUUM_STORAGE_PATH) { $env:CONTINUUM_STORAGE_PATH } else { Join-Path $env:USERPROFILE '.continuum' }
    $buildDir    = Join-Path $cacheRoot 'cache\llama-server-build'

    # Submodule presence: a fresh clone may not have it checked out. Init from our
    # fork (github.com/CambrianTech/llama.cpp) rather than failing.
    if (-not (Test-Path $serverCMake)) {
        Module-Start 'llama-server' 'initializing core/vendor/llama.cpp submodule'
        Push-Location $RepoRoot
        try { Invoke-InstallerProcess -OwnProcessTree 'git' @('submodule', 'update', '--init', 'core/vendor/llama.cpp') } finally { Pop-Location }
    }
    if (-not (Test-Path $serverCMake)) {
        Module-Fail 'llama-server' "llama.cpp submodule missing at $submodule even after init"
    }

    $sourceRevision = (Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $submodule, 'rev-parse', 'HEAD') 2>$null)
    if ($RequireReceipt) {
        $requirement = Get-CoreEngineRequirement -RepoRoot $RepoRoot
        if ($sourceRevision -cne $requirement.source_revision) { throw 'Checked-out llama.cpp differs from the tracked gitlink; refusing receipt migration.' }
    }
    $head = (Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $submodule, 'rev-parse', '--short', 'HEAD') 2>$null)
    if (-not $head) { $head = 'unknown' }

    # Backend: NVIDIA -> CUDA (matches core/llama/build.rs gating), else CPU.
    $backend = Get-CoreEngineBackend; $backendDefs = @()
    if ($backend -eq 'cuda') {
        $build = (Get-ManifestModule 'build-core').build
        $backendDefs = @('-DGGML_CUDA=ON', "-DCMAKE_CUDA_ARCHITECTURES=$($build.cuda_arch)")
    }
    $stampWant = "${head}:${backend}"

    if ((Test-Path $installBin) -and (Test-Path $stampFile) -and
        ((Get-Content $stampFile -Raw -ErrorAction SilentlyContinue).Trim() -eq $stampWant)) {
        if (Test-Path -LiteralPath (Join-Path $installDir 'engine-install.pending')) { throw 'Engine publication is incomplete; rebuild before reuse.' }
        # Legacy stamps still serve normally; they do not acquire a new receipt.
        if (Test-Path -LiteralPath (Join-Path $installDir 'engine-install.json')) {
            $existingReceipt = Get-CoreEngineReceipt -Directory $installDir
            if ($existingReceipt.source_revision -cne $sourceRevision -or $existingReceipt.backend -cne $backend) { throw 'Engine receipt differs from requested build.' }
        }
        if (-not $RequireReceipt -or (Test-Path -LiteralPath (Join-Path $installDir 'engine-install.json'))) {
            Module-Skip 'llama-server' "already current at $installBin ($stampWant)"
            return
        }
    }

    # A new core slot does not require recompiling an unchanged engine. Reuse
    # only a stamped matching build and verify its copy before publishing stamp.
    $engineRoot = Join-Path (Get-ManagedPayloadRoot) 'bin'
    foreach ($sourceDir in @($engineRoot, (Join-Path $engineRoot 'engine-a'),
        (Join-Path $engineRoot 'engine-b'), (Join-Path $engineRoot 'engine-c'))) {
        if ([IO.Path]::GetFullPath($sourceDir) -eq [IO.Path]::GetFullPath($installDir)) { continue }
        $sourceBin = Join-Path $sourceDir 'llama-server.exe'
        $sourceStamp = Join-Path $sourceDir '.llama-server.stamp'
        if (Test-Path -LiteralPath (Join-Path $sourceDir 'engine-install.pending')) { continue }
        if ((Test-Path $sourceBin) -and (Test-Path $sourceStamp) -and
            (Get-Content $sourceStamp -Raw).Trim() -eq $stampWant) {
            # A receipted engine copies its entire verified application namespace.
            $sourceReceipt = Join-Path $sourceDir 'engine-install.json'
            if ($RequireReceipt -and -not (Test-Path -LiteralPath $sourceReceipt)) { continue }
            New-Item -ItemType Directory -Force -Path $installDir | Out-Null
            if (Test-Path -LiteralPath $sourceReceipt) {
                $receipt = Get-CoreEngineReceipt -Directory $sourceDir
                if ($receipt.source_revision -cne $sourceRevision -or $receipt.backend -cne $backend) { throw 'Source engine receipt differs from requested build.' }
                Start-CoreEnginePublication -Directory $installDir
                foreach ($entry in $receipt.files.PSObject.Properties) {
                    Copy-Item -LiteralPath (Join-Path $sourceDir $entry.Name) -Destination (Join-Path $installDir $entry.Name) -Force -ErrorAction Stop
                }
                Copy-CoreEngineReceipt -SourceDirectory $sourceDir -Directory $installDir
                Get-CoreEngineReceipt -Directory $installDir | Out-Null
            } else {
                if ((Test-Path -LiteralPath (Join-Path $installDir 'engine-install.json')) -or (Test-Path -LiteralPath (Join-Path $installDir 'engine-install.pending'))) { throw 'Cannot overwrite a receipted engine with an unreceipted build.' }
                Copy-Item -LiteralPath $sourceBin -Destination $installBin -Force -ErrorAction Stop
            }
            if ((Get-FileHash $sourceBin).Hash -ne (Get-FileHash $installBin).Hash) { throw 'Staged inference engine hash mismatch.' }
            Set-Content -LiteralPath $stampFile -Value $stampWant -Encoding ASCII
            Module-Skip 'llama-server' "staged matching engine ($stampWant) without rebuilding"
            return
        }
    }

    Module-Start 'llama-server' "building llama-server ($backend, llama.cpp@$head) -- the serving-lane child"
    if ($RequireReceipt) {
        Mod-CMake -ExistingOnly
        Mod-CUDA -ExistingOnly
    }
    New-Item -ItemType Directory -Force $buildDir, $installDir | Out-Null

    # Generator: the "Visual Studio 17 2022" generator needs the CUDA VS MSBuild
    # integration (CUDA*.props in the VC BuildCustomizations dir) to enable_language
    # (CUDA) -- but our NO-ADMIN CUDA redist doesn't ship it (it's a full-installer
    # component that writes into Program Files). Ninja drives nvcc DIRECTLY, so it
    # needs zero VS integration -- the robust no-admin CUDA path. Provision ninja
    # (a single ~500KB binary, no admin) and build with it inside the vcvars env
    # (Enter-MsvcEnv puts cl.exe on PATH for nvcc's host side).
    $ninjaDir = Join-Path (Get-ManagedPayloadRoot) 'tools\ninja'
    $ninja = Join-Path $ninjaDir 'ninja.exe'
    if ($backend -eq 'cuda' -and -not (Test-Path $ninja)) {
        Write-Step '  llama-server: fetching ninja (no-admin CUDA build driver)'
        New-Item -ItemType Directory -Force $ninjaDir | Out-Null
        $nz = Join-Path $env:TEMP 'ninja-win.zip'
        Invoke-WebRequest -Uri 'https://github.com/ninja-build/ninja/releases/download/v1.12.1/ninja-win.zip' -OutFile $nz -UseBasicParsing
        Expand-Archive -Path $nz -DestinationPath $ninjaDir -Force
        Remove-Item $nz -ErrorAction SilentlyContinue
    }

    $cmakeArgs = @('-S', $submodule, '-B', $buildDir,
        '-DCMAKE_BUILD_TYPE=Release',
        '-DLLAMA_BUILD_SERVER=ON', '-DLLAMA_BUILD_TOOLS=ON', '-DLLAMA_BUILD_COMMON=ON',
        '-DLLAMA_BUILD_TESTS=OFF', '-DLLAMA_BUILD_EXAMPLES=OFF',
        # We serve local GGUF paths (-m), never fetch by URL -> drop libcurl.
        '-DLLAMA_CURL=OFF',
        # STATIC libs: link ggml/ggml-base/ggml-cuda/llama INTO llama-server.exe
        # (mirrors core/llama/build.rs). Without this, llama-server.exe dynamically
        # links ggml-base.dll etc. that live only in the build tree -> copying just
        # the exe to ~/.continuum/bin fails at spawn with "cannot open ggml-base.dll"
        # (live repro 2026-07-24). Static = one self-contained binary (only the CUDA
        # runtime DLLs remain dynamic, and those are on PATH via the toolkit).
        '-DBUILD_SHARED_LIBS=OFF', '-DGGML_BACKEND_DL=OFF', '-DGGML_BACKEND_DIR=',
        "-DGGML_CUDA=$(@{cpu='OFF';cuda='ON'}[$backend])",
        # Static CRT: a standalone child that needs no VC runtime DLLs on a public box.
        '-DCMAKE_POLICY_DEFAULT_CMP0091=NEW', '-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded')
    if ($backend -eq 'cuda') {
        Enter-MsvcEnv                                    # cl.exe on PATH for nvcc host side
        $cmakeArgs += @('-G', 'Ninja', "-DCMAKE_MAKE_PROGRAM=$ninja",
            '-DCMAKE_C_COMPILER=cl', '-DCMAKE_CXX_COMPILER=cl') + $backendDefs
    }

    # Reboots can build from another worktree while sharing the same cache.
    # The shell installer uses this same source-ownership guard.
    Invoke-InstallerProcess 'cmake' @("-DSOURCE_DIR=$submodule", "-DBUILD_DIR=$buildDir", '-P', (Join-Path $PSScriptRoot 'prepare-llama-build.cmake')) -OwnProcessTree
    if ($LASTEXITCODE -ne 0) { Module-Fail 'llama-server' "CMake cache ownership check failed ($LASTEXITCODE); configure aborted" }
    Invoke-InstallerProcess -FilePath 'cmake' -ArgumentList $cmakeArgs -OwnProcessTree
    if ($LASTEXITCODE -ne 0) { Module-Fail 'llama-server' "cmake configure failed ($LASTEXITCODE)" }
    Invoke-InstallerProcess -FilePath 'cmake' -ArgumentList @('--build', $buildDir, '--target', 'llama-server') -OwnProcessTree
    if ($LASTEXITCODE -ne 0) { Module-Fail 'llama-server' "cmake build failed ($LASTEXITCODE)" }

    # Ninja (single-config) emits under bin\; the VS generator would use bin\Release\.
    $builtBin = @(
        (Join-Path $buildDir 'bin\llama-server.exe'),
        (Join-Path $buildDir 'bin\Release\llama-server.exe')
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $builtBin) { Module-Fail 'llama-server' "build finished but llama-server.exe not found under $buildDir\bin" }

    if ((Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $submodule, 'rev-parse', 'HEAD')) -cne $sourceRevision) { throw 'Engine source revision changed during build.' }
    if (Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $submodule, 'status', '--porcelain', '--untracked-files=all')) { throw 'Engine source changed; cannot publish clean-source receipt.' }
    # Receipt capture belongs to this fresh build; never reconstruct it from a stamp.
    $cache = Get-Content -LiteralPath (Join-Path $buildDir 'CMakeCache.txt') -Raw
    foreach ($setting in @('BUILD_SHARED_LIBS:BOOL=OFF', 'GGML_BACKEND_DL:BOOL=OFF', 'GGML_BACKEND_DIR:PATH=')) {
        if ($cache -notmatch ('(?m)^' + [regex]::Escape($setting) + '\r?$')) { throw "Engine cache violates receipt contract: $setting" }
    }
    $cudaExpected = if ($backend -eq 'cuda') { 'ON' } else { 'OFF' }
    foreach ($setting in @("GGML_CUDA:BOOL=$cudaExpected", 'CMAKE_BUILD_TYPE:STRING=Release')) {
        if ($cache -notmatch ('(?m)^' + [regex]::Escape($setting) + '\r?$')) { throw "Engine cache violates receipt contract: $setting" }
    }
    if ($cache -notmatch '(?m)^CMAKE_MSVC_RUNTIME_LIBRARY:(STRING|UNINITIALIZED)=MultiThreaded\r?$') { throw 'Engine CRT is not static.' }
    Assert-CorePreparedPath -Path $installDir -Expected $installDir
    $runtimeNames = @()
    if ($backend -eq 'cuda') { $runtimeNames = @(Get-ChildItem -LiteralPath (Join-Path (Get-CudaToolkitDirectory) 'bin') -File -Filter '*.dll' | ForEach-Object { $_.Name }) }
    foreach ($oldDll in @(Get-ChildItem -LiteralPath $installDir -File -Filter '*.dll')) {
        if ($oldDll.Name -notin $runtimeNames) { throw "Unowned application DLL in engine slot: $($oldDll.Name)" }
    }
    Start-CoreEnginePublication -Directory $installDir
    Copy-Item -Force $builtBin $installBin
    if ($backend -eq 'cuda') {
        # Pin actual installed toolkit inputs, not a claim of archive provenance.
        $runtime = Join-Path (Get-CudaToolkitDirectory) 'bin'
        Assert-CorePreparedPath -Path $runtime -Expected $runtime
        $dlls = @(Get-ChildItem -LiteralPath $runtime -File -Filter '*.dll')
        if (-not $dlls.Count) { throw 'CUDA toolkit has no application runtime DLLs.' }
        foreach ($dll in $dlls) {
            Assert-CorePreparedPath -Path $dll.FullName -Expected $dll.FullName -File
            Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $installDir $dll.Name) -Force -ErrorAction Stop
        }
    }
    # Use the configured toolchain inspector, not another PATH-selected tool.
    if ($cache -notmatch '(?m)^CMAKE_LINKER:FILEPATH=([^\r\n]+)') { throw 'Configured engine linker is unknown.' }
    $dumpbin = Join-Path (Split-Path $Matches[1] -Parent) 'dumpbin.exe'
    if (-not (Test-Path -LiteralPath $dumpbin -PathType Leaf)) { throw 'Configured engine dependency inspector is missing.' }
    # Imported platform DLLs remain the explicit Windows/driver contract.
    Invoke-InstallerProcess 'cmake' @("-DCMAKE_GET_RUNTIME_DEPENDENCIES_COMMAND=$dumpbin", "-DENGINE_DIR=$($installDir.Replace('\','/'))", "-DSYSTEM_DIR=$([Environment]::SystemDirectory.Replace('\','/'))", '-P', (Join-Path $PSScriptRoot 'verify-engine-imports.cmake')) -OwnProcessTree
    if ($LASTEXITCODE -ne 0) { throw 'Engine application imports could not be bounded.' }
    Save-CoreEngineReceipt -Directory $installDir -SourceRevision $sourceRevision -Backend $backend
    Set-Content -Path $stampFile -Value $stampWant -Encoding ASCII
    Module-Done 'llama-server'
    Write-Ok "llama-server -> $installBin ($stampWant) -- the serving daemon spawns this"
}
