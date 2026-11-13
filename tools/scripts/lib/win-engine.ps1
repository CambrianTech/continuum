# The Windows inference engine build: what decides the published engine's bytes, and nothing
# else. Split from win-modules.ps1 (card af648389) so the engine's cache key
# (core-binaries.yml ENGINE_KEY) hashes the engine build, not the whole installer: #5217 changed
# only the core's cargo features in win-modules.ps1, and that invalidated the cached engine and
# cost a ~2 h CUDA rebuild. Dot-sourced by win-modules.ps1; uses its helpers (Mod-CMake, Mod-CUDA,
# Get-ManifestModule) at call time.

function Get-CoreEngineBackend {
    if (Get-Command nvidia-smi -ErrorAction SilentlyContinue) { return 'cuda' }
    return 'cpu'
}

function Get-CoreEngineRequirement {
    param([Parameter(Mandatory = $true)][string]$RepoRoot, [ValidateSet('cpu', 'cuda')][string]$Backend)
    $revision = (Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $RepoRoot, 'rev-parse', 'HEAD:core/vendor/llama.cpp') 2>$null)
    if ($LASTEXITCODE -ne 0 -or $revision -cnotmatch '^[0-9a-f]{40}$') { throw 'Cannot resolve the tracked llama.cpp gitlink.' }
    return [pscustomobject]@{ source_revision = $revision; backend = $(if ($Backend) { $Backend } else { Get-CoreEngineBackend }) }
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
        [switch]$RequireReceipt,
        # The publisher supplies its existing CUDA floor and the shared Rust CPU
        # policy; a GPU-less build host must not select the consumer's backend.
        [string]$PublishedCudaArchitectures,
        [string[]]$PublishedCpuDefinitions = @()
    )

    $published = -not [string]::IsNullOrEmpty($PublishedCudaArchitectures)
    if ($published) {
        if ($PublishedCudaArchitectures -cnotmatch '^[1-9][0-9]*(-real|-virtual)?(;[1-9][0-9]*(-real|-virtual)?)*$' -or
            -not $RequireReceipt -or -not $PublishedCpuDefinitions.Count -or
            $PublishedCpuDefinitions -cnotcontains '-DGGML_NATIVE=OFF' -or
            @($PublishedCpuDefinitions | Where-Object { $_ -cnotmatch '^-DGGML_[A-Z0-9_]+=OFF$' }).Count) {
            throw 'Published engine requires explicit CUDA targets, receipt, and shared portable CPU definitions.'
        }
    } elseif ($PublishedCpuDefinitions.Count) { throw 'Portable publisher definitions require explicit published CUDA targets.' }

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
        $requirement = if ($published) { Get-CoreEngineRequirement -RepoRoot $RepoRoot -Backend cuda } else { Get-CoreEngineRequirement -RepoRoot $RepoRoot }
        if ($sourceRevision -cne $requirement.source_revision) { throw 'Checked-out llama.cpp differs from the tracked gitlink; refusing receipt migration.' }
    }
    $head = (Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $submodule, 'rev-parse', '--short', 'HEAD') 2>$null)
    if (-not $head) { $head = 'unknown' }

    # Backend: NVIDIA -> CUDA (matches core/llama/build.rs gating), else CPU.
    $backend = if ($published) { 'cuda' } else { Get-CoreEngineBackend }; $backendDefs = @()
    if ($backend -eq 'cuda') {
        $build = (Get-ManifestModule 'build-core').build
        $targets = if ($published) { [pscustomobject]@{ CMake = $PublishedCudaArchitectures } } else { Set-CudaTargets }
        $backendDefs = @('-DGGML_CUDA=ON', "-DCMAKE_CUDA_ARCHITECTURES=$($targets.CMake)")
    }
    $stampWant = "${head}:${backend}"
    if ($backend -eq 'cuda') { $stampWant += ":$($targets.CMake)" }
    if ($published) { $stampWant += ':portable-v1' }

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
        if ($published) { Mod-CMake } else { Mod-CMake -ExistingOnly }
        Mod-CUDA -ExistingOnly -BuildHost:$published
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
        Save-InstallerSmallFile -Uri 'https://github.com/ninja-build/ninja/releases/download/v1.12.1/ninja-win.zip' -OutFile $nz
        Expand-InstallerNinjaArchive -Archive $nz -Destination $ninjaDir
        Remove-Item $nz -ErrorAction SilentlyContinue
    }

    $cmakeArgs = @('-S', $submodule, '-B', $buildDir,
        '-DCMAKE_BUILD_TYPE=Release',
        '-DLLAMA_BUILD_SERVER=ON', '-DLLAMA_BUILD_TOOLS=ON', '-DLLAMA_BUILD_COMMON=ON',
        '-DLLAMA_BUILD_TESTS=OFF', '-DLLAMA_BUILD_EXAMPLES=OFF',
        # We serve local GGUF paths (-m), never fetch by URL -> drop libcurl.
        '-DLLAMA_CURL=OFF',
        # No TLS in the engine (localhost, local GGUF): never find or ship an OpenSSL.
        '-DLLAMA_OPENSSL=OFF',
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
    if ($published) { $cmakeArgs += $PublishedCpuDefinitions }
    else { $cmakeArgs += '-DGGML_NATIVE=ON' }
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
    if ($published) {
        foreach ($definition in $PublishedCpuDefinitions) {
            $name = $definition.Substring(2).Split('=')[0]
            if ($cache -notmatch ('(?m)^' + [regex]::Escape($name) + ':(BOOL|UNINITIALIZED)=OFF\r?$')) {
                throw "Published engine CPU cache violates shared policy: $name"
            }
        }
        if ($cache -notmatch ('(?m)^CMAKE_CUDA_ARCHITECTURES:(STRING|UNINITIALIZED)=' + [regex]::Escape($PublishedCudaArchitectures) + '\r?$')) {
            throw 'Published engine CUDA targets differ from the artifact contract.'
        }
    }
    Assert-CorePreparedPath -Path $installDir -Expected $installDir
    $runtimeDirectories = @(Get-CoreRuntimeDirectories -Cuda:($backend -eq 'cuda') -CMakeCache $cache)
    $runtimeNames = @($runtimeDirectories | ForEach-Object { Get-ChildItem -LiteralPath $_ -File -Filter '*.dll' | ForEach-Object { $_.Name } })
    foreach ($oldDll in @(Get-ChildItem -LiteralPath $installDir -File -Filter '*.dll')) {
        if ($oldDll.Name -notin $runtimeNames) { throw "Unowned application DLL in engine slot: $($oldDll.Name)" }
    }
    Start-CoreEnginePublication -Directory $installDir
    Copy-Item -Force $builtBin $installBin
    # Use the configured toolchain inspector, not another PATH-selected tool.
    if ($cache -notmatch '(?m)^CMAKE_LINKER:FILEPATH=([^\r\n]+)') { throw 'Configured engine linker is unknown.' }
    $dumpbin = Join-Path (Split-Path $Matches[1] -Parent) 'dumpbin.exe'
    if (-not (Test-Path -LiteralPath $dumpbin -PathType Leaf)) { throw 'Configured engine dependency inspector is missing.' }
    Copy-CoreRuntimeClosure -Directory $installDir -Executables @($installBin) -Inspector $dumpbin -RuntimeDirectories $runtimeDirectories -OutputNames (Join-Path $installDir 'runtime-imports.txt')
    Set-Content -Path $stampFile -Value $stampWant -Encoding ASCII
    Save-CoreEngineReceipt -Directory $installDir -SourceRevision $sourceRevision -Backend $backend
    Module-Done 'llama-server'
    Write-Ok "llama-server -> $installBin ($stampWant) -- the serving daemon spawns this"
}
