# One application-runtime owner for published core/CLI binaries and engines.
# Windows/NVIDIA drivers remain platform inputs; VC/OpenMP redistributables do not.
function Get-CoreRuntimeDirectories {
    param([switch]$Cuda, [string]$CMakeCache)
    if (-not $env:VCToolsRedistDir) { Enter-MsvcEnv }
    if (-not $env:VCToolsRedistDir) { throw 'Selected MSVC toolchain did not identify its redistributable directory.' }
    $redist = Join-Path $env:VCToolsRedistDir 'x64'
    Assert-CorePreparedPath -Path $redist -Expected $redist
    $directories = @(Get-ChildItem -LiteralPath $redist -Directory | Where-Object {
        $_.Name -match '^Microsoft\.VC[0-9]+\.(CRT|OpenMP)$'
    } | ForEach-Object { $_.FullName })
    if (-not $directories.Count) { throw 'Selected toolchain has no x64 VC/OpenMP redistributables.' }
    if ($Cuda) { $directories += Join-Path (Get-CudaToolkitDirectory) 'bin' }
    # FindOpenSSL selects headers/libraries outside PATH (e.g. the hosted
    # runner's Program Files/OpenSSL). Its adjacent bin directory owns the
    # matching TLS runtime; never substitute a different PATH installation.
    if ($CMakeCache -match '(?m)^OPENSSL_INCLUDE_DIR:PATH=([^\r\n]+)\r?$') {
        $include = ConvertTo-CoreImagePath $Matches[1]
        Assert-CorePreparedPath -Path $include -Expected $include
        if ((Split-Path $include -Leaf) -ine 'include') { throw 'Configured OpenSSL include layout has no known runtime directory.' }
        $opensslBin = Join-Path (Split-Path $include -Parent) 'bin'
        # Static OpenSSL has no DLL directory; unresolved dynamic imports still
        # fail in the shared resolver below, rather than weakening TLS checks.
        if (Test-Path -LiteralPath $opensslBin -PathType Container) { $directories += $opensslBin }
    }
    foreach ($directory in $directories) { Assert-CorePreparedPath -Path $directory -Expected $directory }
    return $directories
}

function Copy-CoreRuntimeClosure {
    param([string]$Directory, [string[]]$Executables, [string]$Inspector,
        [string[]]$RuntimeDirectories, [string]$OutputNames)
    Assert-CorePreparedPath -Path $Directory -Expected $Directory
    Assert-CorePreparedPath -Path $Inspector -Expected $Inspector -File
    foreach ($executable in $Executables) { Assert-CorePreparedPath -Path $executable -Expected $executable -File }
    foreach ($dll in @(Get-ChildItem -LiteralPath $Directory -File -Filter '*.dll')) { Assert-CorePreparedPath -Path $dll.FullName -Expected $dll.FullName -File }
    # Stage known redistributables before dependency resolution: a build host's
    # System32 copy must never hide the requirement on a clean consumer machine.
    $known = @(Get-Content -LiteralPath (Join-Path $PSScriptRoot 'windows-runtime-redistributables.txt'))
    foreach ($name in $known) {
        $sources = @($RuntimeDirectories | ForEach-Object { Join-Path $_ $name } | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf })
        if ($sources.Count -gt 1) { throw "Ambiguous selected-toolchain redistributable: $name" }
        if ($sources.Count -eq 1) {
            Assert-CorePreparedPath -Path $sources[0] -Expected $sources[0] -File
            Assert-CorePreparedPath -Path (Join-Path $Directory $name) -Expected (Join-Path $Directory $name)
            Copy-Item -LiteralPath $sources[0] -Destination (Join-Path $Directory $name) -Force -ErrorAction Stop
            if ((Get-FileHash -LiteralPath $sources[0]).Hash -cne (Get-FileHash -LiteralPath (Join-Path $Directory $name)).Hash) { throw 'Redistributable changed during copy.' }
        }
    }
    $arguments = @("-DCMAKE_GET_RUNTIME_DEPENDENCIES_COMMAND=$Inspector",
        "-DENGINE_DIR=$($Directory.Replace('\','/'))", "-DSYSTEM_DIR=$([Environment]::SystemDirectory.Replace('\','/'))",
        "-DEXECUTABLES=$(($Executables | ForEach-Object { $_.Replace('\','/') }) -join ';')",
        "-DRUNTIME_DIRS=$(($RuntimeDirectories | ForEach-Object { $_.Replace('\','/') }) -join ';')")
    $resolver = Join-Path $PSScriptRoot 'verify-engine-imports.cmake'
    Invoke-InstallerProcess 'cmake' ($arguments + @('-DCAPTURE_IMPORTS=ON', "-DOUTPUT_NAMES=$OutputNames", '-P', $resolver)) -OwnProcessTree
    if ($LASTEXITCODE -ne 0) { throw 'Application runtime closure could not be captured.' }
    $names = @(Get-Content -LiteralPath $OutputNames | Where-Object { $_ })
    if (@($names | Where-Object { $_ -like 'nvrtc64_*.dll' }).Count) {
        # NVRTC loads its builtins by name, outside the PE import table.
        $builtins = @($RuntimeDirectories | ForEach-Object { Get-ChildItem -LiteralPath $_ -File -Filter 'nvrtc-builtins64_*.dll' })
        if (-not $builtins.Count) { throw 'NVRTC runtime is missing its compiler builtins.' }
        foreach ($dll in $builtins) {
            Assert-CorePreparedPath -Path $dll.FullName -Expected $dll.FullName -File
            Assert-CorePreparedPath -Path (Join-Path $Directory $dll.Name) -Expected (Join-Path $Directory $dll.Name)
            Copy-Item -LiteralPath $dll.FullName -Destination (Join-Path $Directory $dll.Name) -Force -ErrorAction Stop
            if ((Get-FileHash -LiteralPath $dll.FullName).Hash -cne (Get-FileHash -LiteralPath (Join-Path $Directory $dll.Name)).Hash) { throw 'NVRTC builtins changed during copy.' }
            $names += $dll.Name
        }
    }
    Invoke-InstallerProcess 'cmake' ($arguments + @('-P', $resolver)) -OwnProcessTree
    if ($LASTEXITCODE -ne 0) { throw 'Staged application runtime imports could not be verified.' }
    $stagedNames = @{}
    foreach ($file in @(Get-ChildItem -LiteralPath $Directory -File -Filter '*.dll')) { $stagedNames[$file.Name.ToLowerInvariant()] = $file.Name }
    $names = @($names | ForEach-Object {
        if (-not $stagedNames.ContainsKey($_.ToLowerInvariant())) { throw 'Captured runtime disappeared before publication.' }
        $stagedNames[$_.ToLowerInvariant()]
    } | Sort-Object -Unique)
    [IO.File]::WriteAllLines($OutputNames, [string[]]$names, [Text.UTF8Encoding]::new($false))
}

function Publish-CoreWindowsRuntime {
    param([string]$Directory)
    Enter-MsvcEnv
    $inspector = (Get-Command dumpbin -ErrorAction Stop).Source
    $runtimeDirectories = @(Get-CoreRuntimeDirectories -Cuda)
    Copy-CoreRuntimeClosure -Directory $Directory -Executables @((Join-Path $Directory 'continuum.exe')) -Inspector $inspector -RuntimeDirectories $runtimeDirectories -OutputNames (Join-Path $Directory 'bootstrap-runtime-libs.txt')
    $executables = @(Get-ChildItem -LiteralPath $Directory -File -Filter '*.exe' | ForEach-Object { $_.FullName })
    Copy-CoreRuntimeClosure -Directory $Directory -Executables $executables -Inspector $inspector -RuntimeDirectories $runtimeDirectories -OutputNames (Join-Path $Directory 'runtime-imports.txt')
    $names = @(Get-ChildItem -LiteralPath $Directory -File -Filter '*.dll' | ForEach-Object { $_.Name } | Sort-Object -Unique)
    [IO.File]::WriteAllLines((Join-Path $Directory 'runtime-libs.txt'), [string[]]$names, [Text.UTF8Encoding]::new($false))
}
