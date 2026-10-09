# Regression for #4056: exercise installed-slot safety and the real hidden
# supervisor without registering tasks or changing the operator's environment.
# Run with Windows PowerShell 5.1 (also the scheduler's production runtime).
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $repo 'tools\scripts\lib\windows-service.ps1')
. (Join-Path $repo 'tools\scripts\lib\windows-prepared.ps1')
. (Join-Path $repo 'tools\scripts\lib\windows-engine-receipt.ps1')
. (Join-Path $repo 'tools\scripts\lib\windows-elevation.ps1')
$nativeInstallerProcess = ${function:Invoke-InstallerProcess}
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('continuum-service-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    # Publisher imports win-modules alone. A fresh runspace must not inherit the
    # service module that used to accidentally supply receipt path normalization.
    $isolatedEngine = Join-Path $scratch 'isolated-engine'
    New-Item -ItemType Directory -Path $isolatedEngine | Out-Null
    [IO.File]::WriteAllText((Join-Path $isolatedEngine 'llama-server.exe'), 'engine fixture')
    $isolated = [PowerShell]::Create()
    try {
        [void]$isolated.AddScript({ param($module, $directory)
            . $module
            $files = Get-CoreEngineFiles -Directory $directory
            if (-not $files.Contains('llama-server.exe')) { throw 'Standalone publisher lost its engine receipt.' }
        }).AddArgument((Join-Path $repo 'tools/scripts/lib/win-modules.ps1')).AddArgument($isolatedEngine)
        $null = $isolated.Invoke()
        if ($isolated.HadErrors) { throw ($isolated.Streams.Error | Out-String) }
    } finally { $isolated.Dispose() }
    Write-Output 'PASS: isolated publisher imports its complete engine receipt dependency'
    # Regression for card68a33e89: no archive member executes before its trusted
    # release transport identity and hash are checked, independent of GPU policy.
    & {
        . (Join-Path $repo 'tools/scripts/lib/windows-prebuilt.ps1')
        . (Join-Path $repo 'tools/scripts/lib/win-modules.ps1')
        function Module-Fail { param($Name,$Fix) throw "$Name $Fix" }
        $tip = '0123456789012345678901234567890123456789'
        $manifest = [pscustomobject]@{ git_sha=$tip; platform='windows-x86_64'; archive='continuum-core-windows-x86_64.tar.gz'; sha256=('a' * 64); runtime_libs=@('VCOMP140.DLL'); bootstrap_runtime_libs=@('VCOMP140.DLL') }
        Assert-CoreBootstrapManifest $manifest $tip 'windows-x86_64'
        foreach ($case in @(@('git_sha','ffffffffffffffffffffffffffffffffffffffff'), @('platform','macos-arm64'), @('archive','../escape.tar.gz'), @('sha256','short'))) {
            $original = $manifest.($case[0]); $manifest.($case[0]) = $case[1]
            $refused = $false
            try { Assert-CoreBootstrapManifest $manifest $tip 'windows-x86_64' } catch { $refused = $true }
            $manifest.($case[0]) = $original
            if (-not $refused) { throw "Untrusted bootstrap $($case[0]) was accepted" }
        }
        $transportRoot = Join-Path $scratch 'bootstrap-transport'
        $package = Join-Path $transportRoot 'continuum-core-windows-x86_64'
        New-Item -ItemType Directory -Path $package -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $package 'continuum.exe'), 'fixture CLI bytes')
        [IO.File]::WriteAllText((Join-Path $package 'VCOMP140.DLL'), 'fixture OpenMP runtime')
        $archive = Join-Path $transportRoot $manifest.archive
        & tar.exe -czf $archive -C $transportRoot 'continuum-core-windows-x86_64'
        if ($LASTEXITCODE -ne 0) { throw 'Cannot package bootstrap adapter fixture.' }
        $manifest.sha256 = (Get-FileHash -LiteralPath $archive).Hash
        $bootstrap = Join-Path $transportRoot 'selected'
        $selected = Expand-CoreBootstrap $archive $manifest $bootstrap
        if ([IO.File]::ReadAllText($selected) -cne 'fixture CLI bytes' -or [IO.File]::ReadAllText((Join-Path $bootstrap 'VCOMP140.DLL')) -cne 'fixture OpenMP runtime') { throw 'Bootstrap did not stage its exact declared runtime.' }
        $manifest.bootstrap_runtime_libs = @('VCOMP140.DLL','vcomp140.dll')
        $refused = $false
        try { Assert-CoreBootstrapManifest $manifest $tip 'windows-x86_64' } catch { $refused = $true }
        if (-not $refused) { throw 'Case-colliding bootstrap DLLs were accepted.' }
    }
    Write-Output 'PASS: bootstrap refuses wrong revision/platform/path/checksum before execution'
    # what this catches: partial robocopy failure was reported as success, and
    # reruns skipped its existing destination then published an incomplete cache.
    & {
        . (Join-Path $repo 'tools\scripts\lib\win-modules.ps1')
        $savedProfile = $env:USERPROFILE
        try {
            $env:USERPROFILE = Join-Path $scratch 'migration-profile'
            $cold = Join-Path $scratch 'migration-cold'
            $src = Join-Path $env:USERPROFILE '.cache\huggingface'
            $dst = Join-Path $cold 'huggingface'
            $config = Join-Path $env:USERPROFILE '.continuum\config.env'
            $pending = Join-Path (Split-Path $config) 'cold-storage.pending'
            New-Item -ItemType Directory -Force $src, $cold, (Split-Path $config) | Out-Null
            [IO.File]::WriteAllText($config, '# preserve until migration succeeds')
            [IO.File]::WriteAllText($pending, $cold + "`n")
            Set-Content -LiteralPath (Join-Path $src 'first') -Value first
            Set-Content -LiteralPath (Join-Path $src 'second') -Value second
            $script:failColdCopy = $true
            $script:coldExports = 0
            function robocopy {
                if ($script:failColdCopy) {
                    New-Item -ItemType Directory -Force $args[1] | Out-Null
                    Move-Item -LiteralPath (Join-Path $args[0] 'first') -Destination (Join-Path $args[1] 'first')
                    $global:LASTEXITCODE = 8
                } else { & $nativeInstallerProcess (Join-Path $env:SystemRoot 'System32\robocopy.exe') $args }
            }
            function Invoke-InstallerProcess {
                param($FilePath, $ArgumentList, [switch]$OwnProcessTree, [switch]$PreserveChildrenOnSuccess)
                if ($FilePath -eq 'robocopy') { robocopy @ArgumentList }
                else { & $nativeInstallerProcess $FilePath $ArgumentList -OwnProcessTree:$OwnProcessTree -PreserveChildrenOnSuccess:$PreserveChildrenOnSuccess }
            }
            function Get-ColdDrive { throw 'An interrupted migration selected a different drive' }
            function Module-Start { }
            function Module-Skip { }
            function Module-Done { }
            function Write-Step { }
            function Write-Ok { }
            function Set-ColdStorageEnv { param($ColdRoot) $script:coldExports++; Update-ColdStorageConfig -Path $config -ColdRoot $ColdRoot }
            $LASTEXITCODE = 0
            $refused = $false
            try { Mod-ColdStorage } catch { $refused = $_ -match 'robocopy exit 8' }
            if (-not $refused -or $script:coldExports -ne 0 -or [IO.File]::ReadAllText($config) -cne '# preserve until migration succeeds') { throw 'Failed cold move published partial storage' }
            if (-not (Test-Path -LiteralPath ($dst + '.continuum-migration'))) { throw 'Failed migration lost ownership receipt' }
            $script:failColdCopy = $false
            Mod-ColdStorage
            if ($script:coldExports -ne 1 -or (Test-Path -LiteralPath $src) -or (Test-Path -LiteralPath $pending)) { throw 'Owned interrupted migration did not converge' }
            foreach ($name in @('first','second')) { if ((Get-Content -LiteralPath (Join-Path $dst $name)) -ne $name) { throw 'Migration lost cache data' } }
            Mod-ColdStorage
            $src = Join-Path $env:USERPROFILE '.continuum\genome'; $dst = Join-Path $cold 'genome'
            New-Item -ItemType Directory -Force $src, $dst | Out-Null
            Set-Content -LiteralPath (Join-Path $dst 'unrelated') -Value kept
            $refused = $false
            try { Move-ColdDir $src $dst -ColdRoot $cold } catch { $refused = $_ -match 'without an ownership receipt' }
            if (-not $refused -or (Get-Content -LiteralPath (Join-Path $dst 'unrelated')) -ne 'kept') { throw 'Unowned destination was overwritten or accepted' }
            $refused = $false
            try { Move-ColdDir $scratch $dst -ColdRoot $cold } catch { $refused = $_ -match 'outside the selected cache roots' }
            if (-not $refused) { throw 'Migration accepted a source outside the allowed cache roots' }
            $foreign = Join-Path $scratch 'foreign-cache'
            New-Item -ItemType Directory -Force (Join-Path $foreign 'huggingface') | Out-Null
            Set-Content -LiteralPath (Join-Path $foreign 'huggingface\keep') -Value untouched
            # The earlier successful move left an empty .cache parent.
            Remove-Item -LiteralPath (Join-Path $env:USERPROFILE '.cache') -Force
            New-Item -ItemType Junction -Path (Join-Path $env:USERPROFILE '.cache') -Target $foreign | Out-Null
            try {
                $refused = $false
                try { Move-ColdDir (Join-Path $env:USERPROFILE '.cache\huggingface') (Join-Path $cold 'huggingface') -ColdRoot $cold } catch { $refused = $_ -match 'traverses a link' }
                if (-not $refused -or (Get-Content -LiteralPath (Join-Path $foreign 'huggingface\keep')) -ne 'untouched') { throw 'Linked source ancestor exposed foreign files to migration' }
            } finally { [IO.Directory]::Delete((Join-Path $env:USERPROFILE '.cache')) }
            $linkedCold = Join-Path $scratch 'linked-cold'
            New-Item -ItemType Junction -Path $linkedCold -Target $cold | Out-Null
            try {
                $refused = $false
                try { Move-ColdDir (Join-Path $env:USERPROFILE '.cache\huggingface') (Join-Path $linkedCold 'huggingface') -ColdRoot $linkedCold } catch { $refused = $_ -match 'traverses a link' }
                if (-not $refused) { throw 'Absent source bypassed linked destination guard' }
            } finally { [IO.Directory]::Delete($linkedCold) }
        } finally { $env:USERPROFILE = $savedProfile }
        Write-Output 'PASS: interrupted cold migration resumes its owned drive, preserves data/config and refuses unrelated paths'
    }
    # what this catches: the real public entry must persist payload placement
    # BEFORE any prerequisite acquisition. Stop at that boundary, not at a
    # replacement installer, and keep all state in the isolated profile.
    & {
        $entryRepo = Join-Path $scratch 'payload entry'
        $entryLib = Join-Path $entryRepo 'tools\scripts\lib'
        $entryProfile = Join-Path $scratch 'payload entry profile'
        $entryCold = Join-Path $scratch 'payload entry cold'
        New-Item -ItemType Directory -Path $entryLib, $entryProfile -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $repo 'install.ps1') -Destination $entryRepo
        foreach ($name in @('install-common.ps1', 'windows-elevation.ps1', 'windows-service.ps1', 'windows-prepared.ps1', 'windows-prebuilt.ps1', 'payload-paths.ps1')) {
            Copy-Item -LiteralPath (Join-Path $repo "tools\scripts\lib\$name") -Destination $entryLib
        }
        $entryGenerated = Join-Path (Split-Path $entryLib) 'generated'
        New-Item -ItemType Directory -Path $entryGenerated | Out-Null
        Copy-Item -LiteralPath (Join-Path $repo 'tools\scripts\generated\manifest.windows.ps1') -Destination $entryGenerated
        $modules = @'
function Mod-ColdStorage { $env:CONTINUUM_STORAGE_PATH = '__COLD__' }
function Test-WingetAvailable {
    $selected = Get-ManagedPayloadRoot
    if (-not $selected.StartsWith($env:CONTINUUM_STORAGE_PATH + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'payload entry selected the wrong root' }
    throw 'public-payload-stop-before-prerequisites'
}
'@
        $modules.Replace('__COLD__', $entryCold.Replace("'", "''")) | Set-Content -LiteralPath (Join-Path $entryLib 'win-modules.ps1')
        $savedProfile = $env:USERPROFILE
        try {
            $env:USERPROFILE = $entryProfile
            $ErrorActionPreference = 'Continue'
            $entryCommand = "try { & '" + (Join-Path $entryRepo 'install.ps1').Replace("'", "''") + "' } catch { [Console]::Error.WriteLine(`$_.ToString()); exit 1 }"
            $output = (& $nativeInstallerProcess "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($entryCommand))) 2>&1 | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
            $code = $LASTEXITCODE
            $ErrorActionPreference = 'Stop'
            if ($code -eq 0 -or $output -notmatch 'public-payload-stop-before-prerequisites') { throw "Public entry did not reach the checked prerequisite boundary: $output" }
            if (-not (Test-Path -LiteralPath (Join-Path $entryProfile '.continuum\payload-root'))) { throw 'Public entry did not persist payload placement' }
        } finally { $env:USERPROFILE = $savedProfile; $ErrorActionPreference = 'Stop' }
        Write-Output 'PASS: ordinary public installer selects durable cold payloads before prerequisite acquisition'
    }
    # what this catches: a new cold install and a legacy upgrade must persist
    # different placement decisions; losing the cold drive must fail closed.
    & {
        . (Join-Path $repo 'tools\scripts\lib\payload-paths.ps1')
        $homeRoot = Join-Path $scratch 'payload-home'
        $cold = Join-Path $scratch 'payload-cold'
        $selected = Initialize-ManagedPayloadRoot -HomeRoot $homeRoot -ColdRoot $cold
        if ((Split-Path $selected) -ne (Join-Path $cold 'payloads')) { throw 'Fresh payloads did not select scoped cold storage' }
        $other = Initialize-ManagedPayloadRoot -HomeRoot (Join-Path $scratch 'other-home') -ColdRoot $cold
        if ($other -eq $selected) { throw 'Independent homes share installed payloads' }
        if ((Initialize-ManagedPayloadRoot -HomeRoot $homeRoot -ColdRoot (Join-Path $scratch 'different')) -ne $selected) { throw 'Rerun moved payloads' }
        $legacy = Join-Path $scratch 'payload-legacy'
        New-Item -ItemType Directory -Path (Join-Path $legacy 'bin') -Force | Out-Null
        if ((Initialize-ManagedPayloadRoot -HomeRoot $legacy -ColdRoot $cold) -ne $legacy) { throw 'Existing payloads moved implicitly' }
        [IO.File]::WriteAllText((Join-Path $homeRoot 'payload-root'), (Join-Path $scratch 'absent-drive'))
        $refused = $false
        try { $null = Get-ManagedPayloadRoot -HomeRoot $homeRoot } catch { $refused = $true }
        if (-not $refused) { throw 'Missing payload drive silently fell back' }
        Write-Output 'PASS: fresh cold payload placement, sticky rerun, legacy preservation and missing-drive refusal'
    }
    # what this catches: a long-lived desktop inherited no Rust-home settings
    # from an earlier install, and PATH refresh discarded session-selected tools.
    & {
        . (Join-Path $repo 'tools\scripts\lib\install-common.ps1')
        $savedCargo = $env:CARGO_HOME; $savedRustup = $env:RUSTUP_HOME; $savedPath = $env:PATH
        try {
            $env:CARGO_HOME = $null; $env:RUSTUP_HOME = $null
            $env:PATH = 'Z:\fixture tools;z:\FIXTURE TOOLS;' + $savedPath
            Initialize-InstallEnvironment -UserEnvironment @{CARGO_HOME='Z:\user cargo'} -MachineEnvironment @{CARGO_HOME='Z:\machine cargo';RUSTUP_HOME='Z:\machine rustup'}
            if ($env:CARGO_HOME -cne 'Z:\user cargo' -or $env:RUSTUP_HOME -cne 'Z:\machine rustup') { throw 'Registered Rust homes were not restored with user precedence' }
            if (-not $env:PATH.StartsWith('Z:\fixture tools;') -or @($env:PATH -split ';' | Where-Object { $_ -ieq 'Z:\fixture tools' }).Count -ne 1) { throw 'Session PATH was lost or duplicated' }
            $firstPath = $env:PATH
            Initialize-InstallEnvironment -UserEnvironment @{CARGO_HOME='Z:\different cargo'} -MachineEnvironment @{}
            if ($env:CARGO_HOME -cne 'Z:\user cargo' -or $env:PATH -cne $firstPath) { throw 'Environment rerun overwrote explicit values or grew PATH' }
            $env:CARGO_HOME = $null; $env:RUSTUP_HOME = $null
            Initialize-InstallEnvironment -UserEnvironment @{} -MachineEnvironment @{}
            if ($env:CARGO_HOME -or $env:RUSTUP_HOME) { throw 'Unconfigured host acquired invented Rust homes' }
        } finally { $env:CARGO_HOME=$savedCargo; $env:RUSTUP_HOME=$savedRustup; $env:PATH=$savedPath }
        Write-Output 'PASS: persisted Rust homes, explicit process precedence, missing settings and stable session PATH restoration'
    }
    # what this catches: failed prerequisite installs used to warn and continue
    # into builds, while a caller-local status could hide the native result.
    & {
        . (Join-Path $repo 'tools\scripts\lib\install-common.ps1')
        $script:prerequisiteExit = 0
        $script:prerequisiteInstalled = $false
        $script:prerequisiteProbePass = $false
        $script:prerequisiteCalls = 0
        $script:prerequisiteDone = $false
        function winget {
            $script:prerequisiteCalls++
            & $nativeInstallerProcess $env:ComSpec -RawArguments "/d /c exit $script:prerequisiteExit"
            $script:prerequisiteInstalled = $true
        }
        function Invoke-InstallerProcess {
            param($FilePath, $ArgumentList, [switch]$OwnProcessTree, [switch]$PreserveChildrenOnSuccess)
            if ($FilePath -eq 'winget') { winget @ArgumentList }
            else { & $nativeInstallerProcess $FilePath $ArgumentList -OwnProcessTree:$OwnProcessTree -PreserveChildrenOnSuccess:$PreserveChildrenOnSuccess }
        }
        function Update-SessionPath { }
        function Module-Start { }
        function Module-Skip { }
        function Module-Done { $script:prerequisiteDone = $true }
        function Write-Warn2 { }
        $probe = { $script:prerequisiteInstalled -and $script:prerequisiteProbePass }
        $LASTEXITCODE = 73
        foreach ($case in @(@(1603, $false, 'winget exited 1603'), @(0, $false, 'verification probe still fails'), @(0, $true, ''), @(3010, $true, ''))) {
            $script:prerequisiteExit = $case[0]
            $script:prerequisiteProbePass = $case[1]
            $script:prerequisiteInstalled = $false
            $script:prerequisiteDone = $false
            $failure = ''
            try { Install-IfMissing -Name fixture -WingetId fixture.invalid -TestCmd $probe -UserScope }
            catch { $failure = $_.Exception.Message }
            if ($case[2]) {
                if (-not $failure.Contains($case[2]) -or $script:prerequisiteDone) { throw "Prerequisite failure was masked: $failure" }
            } elseif ($failure -or -not $script:prerequisiteDone) { throw "Verified prerequisite was falsely refused: $failure" }
        }
        $before = $script:prerequisiteCalls
        Install-IfMissing -Name fixture -WingetId fixture.invalid -TestCmd $probe -UserScope
        if ($script:prerequisiteCalls -ne $before) { throw 'Already healthy prerequisite was installed again' }
        Write-Output 'PASS: native prerequisite failures and failed verification stop setup; verified success/reboot/reuse remain valid'
    }
    # what this catches: cold-storage reruns erased unrelated config and treated
    # the installer's own quoted path as absent, rediscovering a different disk.
    & {
        . (Join-Path $repo 'tools\scripts\lib\win-modules.ps1')
        $savedProfile = $env:USERPROFILE
        try {
            $env:USERPROFILE = Join-Path $scratch 'cold-profile'
            $config = Join-Path $env:USERPROFILE '.continuum\config.env'
            New-Item -ItemType Directory -Force (Split-Path $config) | Out-Null
            $cold = Join-Path $scratch ('cold cache $literal ' + [char]0x03bb)
            New-Item -ItemType Directory -Force $cold | Out-Null
            $unrelated = "# kept comment`r`nCUSTOM_VALUE='literal $& \value = stays'`r`nLABEL='" + [char]0x03bb + "'`r`n"
            [IO.File]::WriteAllText($config, $unrelated + "CONTINUUM_STORAGE_PATH='old'`r`nHF_HOME='old'`r`n")
            $acl = Get-Acl -LiteralPath $config
            $acl.SetAccessRuleProtection($true, $true)
            Set-Acl -LiteralPath $config -AclObject $acl
            $originalAccess = (Get-Acl -LiteralPath $config).GetSecurityDescriptorSddlForm('Access')
            Update-ColdStorageConfig -Path $config -ColdRoot $cold
            $first = [IO.File]::ReadAllText($config)
            if ((Get-Acl -LiteralPath $config).GetSecurityDescriptorSddlForm('Access') -cne $originalAccess) { throw 'Cold-storage config replacement changed explicit file access policy' }
            if (-not $first.StartsWith($unrelated)) { throw 'Cold-storage setup erased or altered unrelated UTF-8 configuration' }
            if (-not $first.Contains("CONTINUUM_STORAGE_PATH='$cold'")) { throw 'Cold-storage path was not preserved literally' }
            Update-ColdStorageConfig -Path $config -ColdRoot $cold
            if ([IO.File]::ReadAllText($config) -cne $first) { throw 'Cold-storage config rerun was not idempotent' }
            $refused = $false
            try { Update-ColdStorageConfig -Path $config -ColdRoot "bad'path" } catch { $refused = $true }
            if (-not $refused -or [IO.File]::ReadAllText($config) -cne $first) { throw 'Unrepresentable path changed config' }
            function Get-ColdDrive { throw 'Quoted existing cold-storage path triggered drive rediscovery' }
            function Module-Skip { }
            function Set-ColdStorageEnv { param($ColdRoot) if ($ColdRoot -cne $cold) { throw 'Rerun changed the configured path' } }
            Mod-ColdStorage
            [IO.File]::WriteAllText($config, '# no storage key or final newline')
            Update-ColdStorageConfig -Path $config -ColdRoot $cold
            if (-not ([IO.File]::ReadAllText($config)).StartsWith("# no storage key or final newline`nCONTINUUM_STORAGE_PATH=")) { throw 'Appending storage keys damaged existing final line' }
            $entry = [IO.File]::ReadAllText((Join-Path $repo 'install.ps1'))
            if ($entry.IndexOf('    Mod-ColdStorage') -gt $entry.IndexOf('    Test-WingetAvailable')) { throw 'Cold storage is selected after prerequisite downloads' }
        } finally { $env:USERPROFILE = $savedProfile }
        Write-Output 'PASS: cold-storage config preservation, quoted rerun, literal paths and early selection'
    }
    # what this catches: Continuum must delegate firewall verification to AIRC
    # without inventing a broad rule, swallowing failure, or losing owner/path.
    & {
        . (Join-Path $repo 'tools\scripts\lib\win-modules.ps1')
        $binary = Join-Path $scratch "airc O'Brien.exe"
        Set-Content -LiteralPath $binary -Value fixture
        $script:aircSetupCalls = 0
        $script:aircSetupFail = $false
        $savedContext = $env:CAMBRIAN_INSTALL_ELEVATION
        $env:CAMBRIAN_INSTALL_ELEVATION = 'fixture-owner-preserved'
        function Get-Command { param($Name, $ErrorAction) if ($Name -eq 'airc') { return [pscustomobject]@{Source=$binary} }; Microsoft.PowerShell.Core\Get-Command @PSBoundParameters }
        function Module-Skip { }
        function Module-Start { }
        function Module-Done { }
        function Get-ManifestModule { param($Name) if ($Name -ne 'airc') { throw 'Wrong dependency descriptor' }; @{source=@{url='https://fixture.invalid/airc/install.ps1'}} }
        function Save-InstallerSmallFile {
            param($Uri,$OutFile)
            if ($Uri -ne 'https://fixture.invalid/airc/install.ps1') { throw 'Manifest URL ignored' }
            $script:aircSetupCalls++
            $code = @'
param([switch]$FirewallOnly,[string]$AircPath)
if (-not $FirewallOnly -or -not (Test-Path -LiteralPath $AircPath) -or $env:CAMBRIAN_INSTALL_ELEVATION -ne 'fixture-owner-preserved') { exit 91 }
'@
            if ($script:aircSetupFail) { $code += "`nexit 73" } else { $code += "`nexit 0" }
            [IO.File]::WriteAllText($OutFile,$code)
        }
        try {
            Mod-AircFirewall
            if ($script:aircSetupCalls -ne 0) { throw 'Local-only install invoked firewall setup' }
            Mod-AircFirewall -WantsGrid
            if ($script:aircSetupCalls -ne 1) { throw 'Grid setup bypassed canonical AIRC entry' }
            $script:aircSetupFail = $true
            $rejected = $false
            try { Mod-AircFirewall -WantsGrid } catch { $rejected = $_.Exception.Message -match 'AIRC setup failed' }
            if (-not $rejected) { throw 'Continuum hid AIRC firewall failure' }
        } finally { $env:CAMBRIAN_INSTALL_ELEVATION = $savedContext }
        Write-Output 'PASS: AIRC canonical firewall delegation, manifest URL, path/owner preservation and failure propagation'
    }
    # Regression for cardde2cd06e: fresh native installation must invoke the
    # existing checksum owner and refuse activation when that prerequisite fails.
    & {
        . (Join-Path $repo 'tools/scripts/lib/win-modules.ps1')
        function Module-Start { }
        function Module-Done { }
        $fixtureRepo = Join-Path $scratch 'livekit prerequisite repo'
        $fixtureScript = Join-Path $fixtureRepo 'tools/scripts/install-livekit-windows.ps1'
        New-Item -ItemType Directory -Path (Split-Path $fixtureScript) -Force | Out-Null
        [IO.File]::WriteAllText($fixtureScript, 'exit 0')
        Mod-LiveKit -RepoRoot $fixtureRepo
        [IO.File]::WriteAllText($fixtureScript, 'exit 73')
        $refused = $false
        try { Mod-LiveKit -RepoRoot $fixtureRepo } catch { $refused = $_.Exception.Message -match 'LiveKit runtime setup failed.*73' }
        if (-not $refused) { throw 'LiveKit prerequisite failure was hidden' }
        Write-Output 'PASS: LiveKit delegates to existing runtime installer and propagates prerequisite failure'
    }
    # PDF runtime recovery: an existing but unloadable decoder must request
    # repair, rather than throwing before Mod-Poppler reaches its install path.
    & {
        . (Join-Path $repo 'tools\scripts\lib\win-modules.ps1')
        $brokenRuntime = Join-Path $scratch 'broken-pdf-runtime'
        $brokenBin = Join-Path $brokenRuntime 'Library\bin'
        New-Item -ItemType Directory -Path $brokenBin -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $brokenBin 'pdfinfo.exe') -Value 'damaged executable'
        if (Test-PopplerRuntime -Directory $brokenRuntime -Version '26.09.0') {
            throw 'Unlaunchable PDF runtime was accepted as healthy.'
        }
        Write-Output 'PASS: corrupt PDF decoder reports drift for installer repair'
    }
    # what this catches: binary-only updates left a legacy launcher/descriptor
    # behind even when the running core SHA matched HEAD (5090, 2026-09-29).
    $browserLauncher = Join-Path $scratch 'run-service-hidden.ps1'
    [IO.File]::WriteAllText($browserLauncher, 'legacy launcher')
    $browserRelease = [pscustomobject]@{ launcher = $browserLauncher }
    if (-not (Get-CoreBrowserReleaseDrift -RepoRoot $repo -Release $browserRelease)) {
        throw 'Legacy descriptor falsely converged.'
    }
    $browserRelease | Add-Member -NotePropertyName eyeRoot -NotePropertyValue $repo
    if (-not (Get-CoreBrowserReleaseDrift -RepoRoot $repo -Release $browserRelease)) {
        throw 'Legacy launcher falsely converged with the new root.'
    }
    Copy-Item -LiteralPath (Join-Path $repo 'tools\scripts\run-service-hidden.ps1') -Destination $browserLauncher
    if (Get-CoreBrowserReleaseDrift -RepoRoot $repo -Release $browserRelease) {
        throw 'Verified launcher and root did not converge.'
    }
    Write-Output 'PASS: browser release drift includes descriptor and launcher bytes'
    & {
        # Exercise the migration itself through the existing scheduler seam;
        # no live registration or serving process is touched by this fixture.
        $keptCore = Join-Path $scratch 'kept-core.exe'
        $legacy = [pscustomobject]@{ launcher = $browserLauncher; artifact = $keptCore; engine = 'kept-engine'; cli = 'kept-cli' }
        $script:browserTask = [pscustomobject]@{ Description = ($legacy | ConvertTo-Json -Compress); Actions = @([pscustomobject]@{Arguments = 'legacy'}) }
        $script:refuseBrowserRegistration = $true
        function Get-ScheduledTask { $script:browserTask }
        function Clear-Elevation { }
        function New-CoreServiceRelease {
            param($RepoRoot, $ArtifactDirectory, $EnginePath)
            if ($ArtifactDirectory -ne $scratch -or $EnginePath -ne 'kept-engine') { throw 'Browser migration lost its installed source or engine.' }
            $staged = $legacy | ConvertTo-Json | ConvertFrom-Json
            $staged | Add-Member NoteProperty eyeRoot $RepoRoot
            return $staged
        }
        function Register-CoreServiceRelease {
            param($Release, $RepoRoot, $WorkingDirectory)
            # A real installed core can precede checkout HEAD. Metadata migration
            # must validate that release in its own slot, not demand the new SHA.
            if ($WorkingDirectory -ne $scratch) { throw 'Browser migration compared installed release with checkout HEAD.' }
            if ($script:refuseBrowserRegistration) { throw 'fixture registration refused' }
            if ($Release.artifact -ne $keptCore -or $Release.engine -ne 'kept-engine' -or $Release.cli -ne 'kept-cli') {
                throw 'Migration replaced binary or engine identity.'
            }
            $script:browserTask = [pscustomobject]@{
                Description = ($Release | ConvertTo-Json -Compress)
                Actions = @([pscustomobject]@{Arguments = ('launcher -EyeRoot "{0}"' -f $RepoRoot)})
            }
        }
        $original = $script:browserTask.Description
        $refused = $false
        try { Update-CoreBrowserRelease -RepoRoot $repo } catch { $refused = $_ -match 'fixture registration refused' }
        if (-not $refused -or $script:browserTask.Description -cne $original) { throw 'Registration refusal did not preserve the release.' }
        $script:refuseBrowserRegistration = $false
        Update-CoreBrowserRelease -RepoRoot $repo
        if (Get-CoreBrowserReleaseDrift -RepoRoot $repo) { throw 'Migration did not converge.' }
        # Rust tracked paths and PowerShell fresh-install paths can spell the
        # same Windows directory differently; that must not cause redeploys.
        if (Get-CoreBrowserReleaseDrift -RepoRoot $repo.ToUpperInvariant()) {
            throw 'Path casing caused perpetual browser migration drift.'
        }
        $script:browserTask.Actions[0].Arguments = 'legacy'
        if (-not (Get-CoreBrowserReleaseDrift -RepoRoot $repo)) { throw 'Missing action argument falsely converged.' }
        Update-CoreBrowserRelease -RepoRoot $repo
        if (Get-CoreBrowserReleaseDrift -RepoRoot $repo) { throw 'Action-only drift was not repaired.' }
        Remove-Variable browserTask,refuseBrowserRegistration -Scope Script
    }
    Write-Output 'PASS: browser migration preserves identities, propagates refusal and repairs action drift'
    # Engine application receipts reject changed candidate sets and bytes using
    # real temporary files, without building/installing/spawning an engine.
    $engineFixture = Join-Path $scratch 'receipt engine'
    New-Item -ItemType Directory -Path $engineFixture | Out-Null
    [IO.File]::WriteAllText((Join-Path $engineFixture 'llama-server.exe'), 'engine')
    $runtimeFixture = Join-Path $engineFixture 'cublas64_12.dll'
    [IO.File]::WriteAllText($runtimeFixture, 'before')
    Start-CoreEnginePublication -Directory $engineFixture
    $refused = $false
    try { Get-CoreEngineReceipt -Directory $engineFixture | Out-Null } catch { $refused = $_ -match 'publication is incomplete' }
    if (-not $refused) { throw 'Incomplete fresh engine publication looked legacy.' }
    Save-CoreEngineReceipt -Directory $engineFixture -SourceRevision ('a' * 40) -Backend cuda
    Get-CoreEngineReceipt -Directory $engineFixture | Out-Null
    $newCandidate = Join-Path $engineFixture 'ggml-cuda-new.dll'
    [IO.File]::WriteAllText($newCandidate, 'candidate')
    $refused = $false
    try { Get-CoreEngineReceipt -Directory $engineFixture | Out-Null } catch { $refused = $_ -match 'membership changed' }
    if (-not $refused) { throw 'Added backend candidate was accepted.' }
    Remove-Item -LiteralPath $newCandidate
    $priorTime = (Get-Item -LiteralPath $runtimeFixture).LastWriteTimeUtc
    [IO.File]::WriteAllText($runtimeFixture, 'after!')
    (Get-Item -LiteralPath $runtimeFixture).LastWriteTimeUtc = $priorTime
    $refused = $false
    try { Get-CoreEngineReceipt -Directory $engineFixture | Out-Null } catch { $refused = $_ -match 'input changed' }
    if (-not $refused) { throw 'Same-length backdated runtime replacement was accepted.' }
    [IO.File]::WriteAllText($runtimeFixture, 'before')
    $engineCopy = Join-Path $scratch 'receipt copied slot'
    New-Item -ItemType Directory -Path $engineCopy | Out-Null
    Get-ChildItem -LiteralPath $engineFixture -File | Where-Object { $_.Name -ne 'engine-install.json' } | Copy-Item -Destination $engineCopy
    Start-CoreEnginePublication -Directory $engineCopy
    [IO.File]::WriteAllText((Join-Path $engineCopy 'cublas64_12.dll'), 'broken')
    $refused = $false
    try { Copy-CoreEngineReceipt -SourceDirectory $engineFixture -Directory $engineCopy } catch { $refused = $_ -match 'bytes differ' }
    if (-not $refused -or -not (Test-Path -LiteralPath (Join-Path $engineCopy 'engine-install.pending'))) { throw 'Failed slot copy lost its incomplete state.' }
    Copy-Item -LiteralPath $runtimeFixture -Destination (Join-Path $engineCopy 'cublas64_12.dll') -Force
    Copy-CoreEngineReceipt -SourceDirectory $engineFixture -Directory $engineCopy
    Get-CoreEngineReceipt -Directory $engineCopy | Out-Null
    Remove-Item -LiteralPath (Join-Path $engineCopy 'cublas64_12.dll')
    $refused = $false
    try { Get-CoreEngineReceipt -Directory $engineCopy | Out-Null } catch { $refused = $true }
    if (-not $refused) { throw 'Missing runtime file was accepted.' }
    Write-Output 'PASS: engine receipt pins application bytes, membership and copied-slot inputs'
    # Receipt migration must use the existing source/slot owners even when the
    # source SHA and legacy stamp are already current. No compiler is invoked.
    & {
        . (Join-Path $repo 'tools\scripts\lib\win-modules.ps1')
        function Get-CoreEngineBackend { 'cpu' }
        function git { $global:LASTEXITCODE = 0; if ($args -contains '--short') { 'aaaaaaa' } else { 'a' * 40 } }
        function Invoke-InstallerProcess {
            param($FilePath, $ArgumentList, [switch]$OwnProcessTree, [switch]$PreserveChildrenOnSuccess)
            if ($FilePath -eq 'git') { git @ArgumentList }
            else { & $nativeInstallerProcess $FilePath $ArgumentList -OwnProcessTree:$OwnProcessTree -PreserveChildrenOnSuccess:$PreserveChildrenOnSuccess }
        }
        function Module-Skip { }
        function Module-Start { throw 'fixture: real build branch selected' }
        function Module-Fail { param($Name, $Message) throw $Message }
        $profile = Join-Path $scratch 'migration-profile'
        $sourceRepo = Join-Path $scratch 'migration-source'
        $server = Join-Path $sourceRepo 'core\vendor\llama.cpp\tools\server'
        New-Item -ItemType Directory -Path $server -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $server 'CMakeLists.txt') -Value 'fixture'
        $root = Join-Path $profile '.continuum\bin'
        $source = Join-Path $root 'engine-a'
        $destination = Join-Path $root 'engine-b'
        $third = Join-Path $root 'engine-c'
        New-Item -ItemType Directory -Path $source, $destination, $third -Force | Out-Null
        foreach ($slot in @($source, $destination)) {
            [IO.File]::WriteAllText((Join-Path $slot 'llama-server.exe'), 'legacy')
            Set-Content -LiteralPath (Join-Path $slot '.llama-server.stamp') -Value 'aaaaaaa:cpu'
        }
        $oldProfile = $env:USERPROFILE
        try {
            $env:USERPROFILE = $profile
            $requirement = Get-CoreEngineRequirement -RepoRoot $sourceRepo
            if (-not (Get-CoreEngineDrift -Directory $source -Requirement $requirement)) { throw 'Legacy stamp was called converged.' }
            $refused = $false
            try { Mod-LlamaServer -RepoRoot $sourceRepo -InstallDirectory $destination -RequireReceipt }
            catch { $refused = $_ -match 'real build branch selected' }
            if (-not $refused -or (Test-Path (Join-Path $destination 'engine-install.json')) -or
                [IO.File]::ReadAllText((Join-Path $source 'llama-server.exe')) -cne 'legacy') {
                throw 'Legacy destination/source shortcut bypassed receipt migration or changed the incumbent.'
            }
            # Ordinary compatible reuse retains its prior behavior.
            Mod-LlamaServer -RepoRoot $sourceRepo -InstallDirectory $destination
            Save-CoreEngineReceipt -Directory $source -SourceRevision ('a' * 40) -Backend cpu
            $sourceHash = (Get-FileHash (Join-Path $source 'engine-install.json')).Hash
            Mod-LlamaServer -RepoRoot $sourceRepo -InstallDirectory $destination -RequireReceipt
            if ((Get-CoreEngineDrift -Directory $destination -Requirement $requirement) -or
                (Get-FileHash (Join-Path $destination 'engine-install.json')).Hash -cne $sourceHash) {
                throw 'Verified reuse did not preserve the original receipt.'
            }
            Start-CoreEnginePublication -Directory $source
            Start-CoreEnginePublication -Directory $destination
            $refused = $false
            try { Mod-LlamaServer -RepoRoot $sourceRepo -InstallDirectory $third -RequireReceipt }
            catch { $refused = $_ -match 'real build branch selected' }
            if (-not $refused) { throw 'Pending source was reused.' }
            Remove-Item -LiteralPath (Join-Path $source 'engine-install.pending')
            [IO.File]::WriteAllText((Join-Path $source 'llama-server.exe'), 'broken')
            $refused = $false
            try { Mod-LlamaServer -RepoRoot $sourceRepo -InstallDirectory $third -RequireReceipt }
            catch { $refused = $_ -match 'input changed' }
            if (-not $refused -or (Test-Path (Join-Path $third 'engine-install.json'))) { throw 'Invalid source was recertified.' }
            [IO.File]::WriteAllText((Join-Path $source 'llama-server.exe'), 'legacy')
            Remove-Item -LiteralPath (Join-Path $destination 'engine-install.pending')
            $script:migrationDescription = (@{ engine = (Join-Path $source 'llama-server.exe') } | ConvertTo-Json -Compress)
            function Get-ScheduledTask { [pscustomobject]@{ Description = $script:migrationDescription } }
            function Get-CimInstance { [pscustomobject]@{ Name = 'llama-server.exe'; ExecutablePath = (Join-Path $third 'llama-server.exe') } }
            $receiptPath = Join-Path $scratch 'prepared-engine-path'
            Prepare-CoreServiceEngine -RepoRoot $sourceRepo -Description $script:migrationDescription -ReceiptPath $receiptPath
            if ([IO.File]::ReadAllText($receiptPath) -cne (Join-Path $destination 'llama-server.exe') -or
                (Get-FileHash (Join-Path $source 'engine-install.json')).Hash -cne $sourceHash) {
                throw 'Preparation selected a live/registered engine or changed its receipt.'
            }
            $refused = $false
            try { Prepare-CoreServiceEngine -RepoRoot $sourceRepo -Description '{}' -ReceiptPath $receiptPath }
            catch { $refused = $_ -match 'release changed before' }
            if (-not $refused) { throw 'Stale installed selection reached preparation.' }
        } finally { $env:USERPROFILE = $oldProfile }
    }
    Write-Output 'PASS: receipt migration selects verified reuse or checked build without legacy/pending bypass'
    # The CLI starts a fresh PowerShell, so receipt validation cannot depend on
    # functions that happen to have been dot-sourced by this fixture's parent.
    $coldScript = @"
`$ErrorActionPreference='Stop'
try {
. '$($repo.Replace("'", "''"))/tools/scripts/lib/windows-service.ps1'
. '$($repo.Replace("'", "''"))/tools/scripts/lib/win-modules.ps1'
`$drift=Get-CoreEngineDrift -Directory '$($engineFixture.Replace("'", "''"))' -Requirement ([pscustomobject]@{source_revision='$('a' * 40)';backend='cuda'})
if (`$drift) { throw `$drift }
} catch { [Console]::Error.WriteLine(`$_.ToString()); exit 1 }
"@
    $info = [Diagnostics.ProcessStartInfo]::new((Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'))
    $info.Arguments = '-NoProfile -NonInteractive -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($coldScript))
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $child = [Diagnostics.Process]::Start($info)
    try {
        $stdout = $child.StandardOutput.ReadToEndAsync()
        $stderr = $child.StandardError.ReadToEndAsync()
        if (-not $child.WaitForExit(120000)) { $child.Kill(); $child.WaitForExit(); throw 'Cold engine receipt query timed out.' }
        if ($child.ExitCode -ne 0) { throw "Cold engine receipt query failed: $($stdout.Result) $($stderr.Result)" }
    } finally { $child.Dispose() }
    # The actual handoff function transfers the installer's exclusive lease
    # before invoking reboot. Stop at the scheduler boundary, before PATH writes.
    & {
        $lockPath = Join-Path $scratch 'handoff-install.lock'
        $lease = [IO.File]::Open($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        $marker = Join-Path $scratch 'handoff-acquired'
        $cli = Join-Path $scratch 'handoff-cli.ps1'
        function Invoke-InstallerProcess {
            param($FilePath, $ArgumentList, [switch]$OwnProcessTree, [switch]$PreserveChildrenOnSuccess)
            if ($FilePath -eq $cli) { & $FilePath @ArgumentList }
            else { & $nativeInstallerProcess $FilePath $ArgumentList -OwnProcessTree:$OwnProcessTree -PreserveChildrenOnSuccess:$PreserveChildrenOnSuccess }
        }
        $core = Join-Path $scratch 'handoff-core.exe'
        [IO.File]::WriteAllText($core, 'fixture-core')
        @"
if (`$args -contains '--validate-only') { Write-Output 'continuum-install-lease-protocol:1'; `$global:LASTEXITCODE = 0; return }
if (`$args -notcontains '--service-descriptor-sha') { throw 'Missing selected descriptor binding' }
`$owned = [IO.File]::Open('$($lockPath.Replace("'", "''"))', [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
try { [IO.File]::WriteAllText('$($marker.Replace("'", "''"))', 'acquired') } finally { `$owned.Dispose() }
`$global:LASTEXITCODE = 0
"@ | Set-Content -LiteralPath $cli
        function Get-ScheduledTask {
            $busy = $false
            try { $unexpected = [IO.File]::Open($lockPath, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None); $unexpected.Dispose() }
            catch [IO.IOException] { $busy = $true }
            if (-not $busy) { throw 'Post-handoff tail lost the installation lease.' }
            throw 'fixture: handoff completed before scheduler check'
        }
        try {
            $busy = $false
            try { $unexpected = [IO.File]::Open($lockPath, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None); $unexpected.Dispose() }
            catch [IO.IOException] { $busy = $true }
            if (-not $busy) { throw 'Installer lease was not exclusive before handoff.' }
            $stopped = $false
            try { Invoke-CoreServiceRelease -Release ([pscustomobject]@{ cli = $cli; artifact = $core }) -RepoRoot $scratch -InstallLease $lease }
            catch { $stopped = $_ -match 'handoff completed before scheduler check' }
            if (-not $stopped -or -not (Test-Path -LiteralPath $marker)) { throw 'Reboot did not receive the released installation lease.' }
        } finally { $lease.Dispose() }
        # A legacy prepared CLI cannot inherit an unsupported lock protocol.
        Set-Content -LiteralPath $cli -Value "Write-Output 'prebuilt validated'; `$global:LASTEXITCODE = 0"
        $lease = [IO.File]::Open($lockPath, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
        try {
            $refused = $false
            try { Invoke-CoreServiceRelease -Release ([pscustomobject]@{ cli = $cli; artifact = $core }) -RepoRoot $scratch -InstallLease $lease }
            catch { $refused = $_ -match 'lacks the verified installation lease protocol' }
            $busy = $false
            try { $unexpected = [IO.File]::Open($lockPath, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None); $unexpected.Dispose() }
            catch [IO.IOException] { $busy = $true }
            if (-not $refused -or -not $busy) { throw 'Legacy CLI refusal lost installer ownership.' }
        } finally { $lease.Dispose() }
    }
    Write-Output 'PASS: installer registration-to-reboot lease transfer permits exclusive reacquisition'
    # Regression for 81021ff6: the real scheduler projects SID registration as
    # an account name. Resolve via Windows without broadening caller ownership.
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    foreach ($owner in @($identity.User.Value, $identity.Name, $env:USERNAME)) {
        if (-not (Test-CoreTaskUser -UserId $owner -ExpectedSid $identity.User.Value)) { throw "Equivalent task owner was refused: $owner" }
    }
    foreach ($owner in @('S-1-5-18', 'S-invalid', ('continuum-unmapped-' + [guid]::NewGuid().ToString('N')), '')) {
        if (Test-CoreTaskUser -UserId $owner -ExpectedSid $identity.User.Value) { throw "Different/unresolved task owner was accepted: $owner" }
    }
    Write-Output 'PASS: scheduler SID/account-name identities compare equally; different/unresolved owners fail closed'
    $bootstrapAcl = [Security.AccessControl.DirectorySecurity]::new()
    $bootstrapAcl.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
    $bootstrapAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($identity.User, 'ReadAndExecute', 'Allow'))
    Assert-CoreBootstrapAccess -Security $bootstrapAcl
    $bootstrapAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($identity.User, 'DeleteSubdirectoriesAndFiles', 'Allow'))
    $refused = $false
    try { Assert-CoreBootstrapAccess -Security $bootstrapAcl } catch { $refused = $_ -match 'writable' }
    if (-not $refused) { throw 'Delete-child access can replace the protected bootstrap' }
    $refused = $false
    try { Get-CoreSupervisorBootstrap -UserSid '..\outside' | Out-Null } catch { $refused = $true }
    if (-not $refused) { throw 'Bootstrap principal escaped its protected path' }
    Write-Output 'PASS: bootstrap boundary rejects caller delete-child authority'
    & {
        $fixed = Join-Path $scratch 'protected-bootstrap.exe'
        Set-Content -LiteralPath $fixed -Value 'existing protected image'
        function Get-CoreSupervisorBootstrap { param($UserSid) $fixed }
        $script:bootstrapVerifications = 0
        function Assert-CoreSupervisorBootstrap { param($Path,$UserSid) $script:bootstrapVerifications++ }
        function Invoke-InstallerProcess { param($Executable,$Arguments) if ($Executable -ne $fixed -or ($Arguments -join ' ') -ne 'installed-service --protocol') { throw 'Unexpected bootstrap probe' }; $global:LASTEXITCODE=0; '2' }
        function Copy-Item { throw 'Task reprovisioning attempted to replace the protected bootstrap' }
        Install-CoreSupervisorBootstrap -Plan ([pscustomobject]@{cli=$fixed;shell=$fixed;userSid=$identity.User.Value;bootstrapSource='missing candidate';bootstrapHashes=@{}})
        if ($script:bootstrapVerifications -ne 1 -or (Get-Content -LiteralPath $fixed -Raw).Trim() -ne 'existing protected image') { throw 'Existing bootstrap was not verified/reused intact' }
    }
    Write-Output 'PASS: task reprovisioning reuses the verified stable bootstrap without replacing loader files'
    # Regression for 72920541: retrying registration must select exact prepared
    # files, never treat an unchecked descriptor as a source-build cache hit.
    & {
        function Write-Step { param($msg) }
        $resumeRoot = Join-Path $scratch 'resume installed'
        $payload = Initialize-ManagedPayloadRoot -HomeRoot $resumeRoot -ColdRoot (Join-Path $scratch 'prepared cold')
        $serviceSlot = Join-Path $payload 'bin\service-a'
        $engineSlot = Join-Path $payload 'bin\engine-a'
        New-Item -ItemType Directory -Path $serviceSlot, $engineSlot, (Join-Path $resumeRoot 'logs') -Force | Out-Null
        $release = [pscustomobject]@{ artifact = (Join-Path $serviceSlot 'continuum-core-server.exe');
            cli = (Join-Path $serviceSlot 'continuum.exe'); launcher = (Join-Path $serviceSlot 'run-service-hidden.ps1');
            engine = (Join-Path $engineSlot 'llama-server.exe'); socket = (Join-Path $scratch 'prepared.sock');
            logDirectory = (Join-Path $resumeRoot 'logs') }
        foreach ($field in @('artifact', 'cli', 'launcher', 'engine')) { Set-Content -LiteralPath $release.$field -Value $field }
        Save-CorePreparedRelease -Release $release -InstallRoot $resumeRoot
        # Re-preparing must atomically replace an existing receipt in both PowerShell hosts.
        Set-Content -LiteralPath $release.cli -Value 'replacement cli'
        Save-CorePreparedRelease -Release $release -InstallRoot $resumeRoot
        $loaded = Get-CorePreparedRelease -InstallRoot $resumeRoot
        if ($loaded.artifact -ne $release.artifact) { throw 'Prepared receipt selected a different release' }
        Set-Content -LiteralPath $release.cli -Value 'tampered'
        $refused = $false
        try { Get-CorePreparedRelease -InstallRoot $resumeRoot | Out-Null } catch { $refused = $_ -match 'changed since preparation' }
        if (-not $refused) { throw 'Changed prepared artifact was accepted' }
        Set-Content -LiteralPath $release.cli -Value 'cli'
        $receiptPath = Join-Path $resumeRoot 'install-prepared.json'
        $receipt = Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json
        $receipt.userSid = 'S-1-5-18'
        $receipt | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $receiptPath
        $refused = $false
        try { Get-CorePreparedRelease -InstallRoot $resumeRoot | Out-Null } catch { $refused = $_ -match 'schema or owner' }
        if (-not $refused) { throw 'Wrong-owner receipt was accepted' }
        Remove-Item -LiteralPath $receiptPath
        $script:resumeTask = [pscustomobject]@{ Principal = [pscustomobject]@{ UserId = [Security.Principal.WindowsIdentity]::GetCurrent().Name };
            Description = ($release | ConvertTo-Json -Compress) }
        function Get-ScheduledTask { $script:resumeTask }
        $loaded = Get-CorePreparedRelease -InstallRoot $resumeRoot
        if ($loaded.engine -ne $release.engine) { throw 'Legacy task release selection changed the engine' }
        $bad = $release | ConvertTo-Json | ConvertFrom-Json
        $bad.cli = Join-Path $scratch 'outside.exe'
        $refused = $false
        try { Assert-CorePreparedRelease -Release $bad -InstallRoot $resumeRoot } catch { $refused = $_ -match 'expected installed layout' }
        if (-not $refused) { throw 'Prepared path escaped the paired slot' }
        $bad = $release | ConvertTo-Json | ConvertFrom-Json
        $bad | Add-Member NoteProperty unknown 'extra'
        $refused = $false
        try { Assert-CorePreparedRelease -Release $bad -InstallRoot $resumeRoot } catch { $refused = $_ -match 'unexpected or missing fields' }
        if (-not $refused) { throw 'Unknown descriptor field was accepted' }
        # Browser-root evolution must work through the prepared-release path,
        # while old descriptors above remain valid and unsafe roots are refused.
        $withEye = $release | ConvertTo-Json | ConvertFrom-Json
        $withEye | Add-Member NoteProperty eyeRoot $repo
        Assert-CorePreparedRelease -Release $withEye -InstallRoot $resumeRoot
        $withEye.eyeRoot = 'relative/assets'
        $refused = $false
        try { Assert-CorePreparedRelease -Release $withEye -InstallRoot $resumeRoot } catch { $refused = $_ -match 'eyeRoot must be absolute' }
        if (-not $refused) { throw 'Relative browser root was accepted' }
        # One preparation owner supplies both pending and committed receipts.
        # Two release selections and a failed-selection rollback leave the fixed
        # scheduler registration untouched; pending preparation cannot select boot.
        $dll = Join-Path $serviceSlot 'fixture-runtime.dll'
        Set-Content -LiteralPath $dll -Value 'runtime bytes'
        Set-Content -LiteralPath (Join-Path $serviceSlot 'runtime-libs.txt') -Value 'fixture-runtime.dll'
        $firstSelection = Initialize-CoreActiveSelection -Task $script:resumeTask -Release $release -InstallRoot $resumeRoot
        if ($firstSelection -or (Get-CorePreparedRelease -InstallRoot $resumeRoot -Selection Active).artifact -ne $release.artifact) { throw 'Legacy migration did not preserve its original selection' }
        $secondSlot = Join-Path $payload 'bin\service-b'
        New-Item -ItemType Directory -Path $secondSlot | Out-Null
        $second = $release | ConvertTo-Json | ConvertFrom-Json
        foreach ($field in @('artifact', 'cli', 'launcher')) {
            $second.$field = Join-Path $secondSlot (Split-Path $release.$field -Leaf)
            Copy-Item -LiteralPath $release.$field -Destination $second.$field
        }
        foreach ($name in @('runtime-libs.txt', 'fixture-runtime.dll')) { Copy-Item -LiteralPath (Join-Path $serviceSlot $name) -Destination (Join-Path $secondSlot $name) }
        Save-CorePreparedRelease -Release $second -InstallRoot $resumeRoot
        if ((Get-CorePreparedRelease -InstallRoot $resumeRoot -Selection Active).artifact -ne $release.artifact) { throw 'Preparation changed active selection' }
        Save-CorePreparedRelease -Release $second -InstallRoot $resumeRoot -Selection Active
        $legacyTask = $script:resumeTask
        $activePath = Join-Path $resumeRoot 'install-active.json'
        $script:resumeTask = [pscustomobject]@{ Principal = $legacyTask.Principal;
            Description = ([ordered]@{schema=2;activeRelease=$activePath;bootstrap=(Get-CoreSupervisorBootstrap)} | ConvertTo-Json -Compress);
            Actions = @([pscustomobject]@{ Execute=(Get-CoreSupervisorBootstrap); WorkingDirectory=(Split-Path (Get-CoreSupervisorBootstrap) -Parent); Arguments=('installed-service core "{0}"' -f $activePath) }) }
        $fixedDescription = $script:resumeTask.Description
        if ((Get-CoreRegisteredRelease -Task $script:resumeTask -InstallRoot $resumeRoot).artifact -ne $second.artifact) { throw 'Fixed supervisor did not resolve second release' }
        Set-Content -LiteralPath (Join-Path $secondSlot 'fixture-runtime.dll') -Value 'tampered runtime'
        $refused = $false
        try { Get-CorePreparedRelease -InstallRoot $resumeRoot -Selection Active | Out-Null } catch { $refused = $_ -match 'changed since preparation' }
        if (-not $refused) { throw 'Changed runtime DLL was accepted' }
        Copy-Item -LiteralPath $dll -Destination (Join-Path $secondSlot 'fixture-runtime.dll') -Force
        $refused = $false
        try { Restore-CoreActiveRelease -ExpectedDescription '{}' -InstallRoot $resumeRoot | Out-Null } catch { $refused = $_ -match 'newer selection' }
        if (-not $refused) { throw 'Rollback overwrote a different active selection' }
        $restored = Restore-CoreActiveRelease -ExpectedDescription ($second | ConvertTo-Json -Compress) -InstallRoot $resumeRoot
        if ($restored.artifact -ne $release.artifact -or $script:resumeTask.Description -cne $fixedDescription) { throw 'Rollback changed supervisor registration or selected wrong release' }
        Clear-CorePreparedSelectionForSlot -InstallRoot $resumeRoot -Slot $secondSlot
        if (Test-Path -LiteralPath $receiptPath) { throw 'Reused inactive slot retained a stale pending receipt' }
        Remove-Item -LiteralPath $activePath
        $refused = $false
        try { Get-CoreRegisteredRelease -Task $script:resumeTask -InstallRoot $resumeRoot | Out-Null } catch { $refused = $true }
        if (-not $refused) { throw 'Missing active receipt fell back to legacy selection' }
        $script:resumeTask = $legacyTask
        Get-ChildItem -LiteralPath $secondSlot -File | ForEach-Object { [IO.File]::Delete($_.FullName) }
        [IO.Directory]::Delete($secondSlot)
        Write-Output 'PASS: active/pending separation, two selections, DLL integrity, compare-and-restore and stale pending invalidation'
        $redirect = Join-Path $payload 'bin\service-b'
        New-Item -ItemType Junction -Path $redirect -Target $serviceSlot | Out-Null
        try {
            $bad = $release | ConvertTo-Json | ConvertFrom-Json
            foreach ($field in @('artifact', 'cli', 'launcher')) { $bad.$field = Join-Path $redirect (Split-Path $bad.$field -Leaf) }
            $refused = $false
            try { Assert-CorePreparedRelease -Release $bad -InstallRoot $resumeRoot } catch { $refused = $_ -match 'redirected' }
            if (-not $refused) { throw 'Prepared slot junction was accepted' }
        } finally { [IO.Directory]::Delete($redirect) }
        $script:resumeOrder = @()
        function Register-CoreServiceRelease {
            param($Release, $RepoRoot, $WorkingDirectory)
            if ($WorkingDirectory -ne $serviceSlot -or $RepoRoot -ne $repo -or $env:CONTINUUM_CORE_SOCKET -ne $release.socket) { throw 'Resume mixed source/installed context or socket' }
            $script:resumeOrder += 'register'
        }
        function Invoke-CoreServiceRelease { param($Release, $RepoRoot, $WorkingDirectory) $script:resumeOrder += 'handoff' }
        $originalSocket = $env:CONTINUUM_CORE_SOCKET
        Resume-CorePreparedRelease -RepoRoot $repo -InstallRoot $resumeRoot
        if (($script:resumeOrder -join ',') -ne 'register,handoff' -or $env:CONTINUUM_CORE_SOCKET -ne $originalSocket) { throw 'Resume ordering or socket restoration failed' }
        function Register-CoreServiceRelease { throw 'fixture registration refused' }
        $script:resumeOrder = @()
        $refused = $false
        try { Resume-CorePreparedRelease -RepoRoot $repo -InstallRoot $resumeRoot } catch { $refused = $_ -match 'fixture registration refused' }
        if (-not $refused -or $script:resumeOrder.Count -ne 0 -or $env:CONTINUUM_CORE_SOCKET -ne $originalSocket) { throw 'Failed registration reached handoff or leaked socket context' }

        # Execute the public installer in a disposable profile, with only the
        # registration/handoff boundary mocked. Loading provisioning is a trap.
        $fakeRepo = Join-Path $scratch 'resume installer'
        $fakeLib = Join-Path $fakeRepo 'tools\scripts\lib'
        New-Item -ItemType Directory -Path $fakeLib -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $repo 'install.ps1') -Destination $fakeRepo
        foreach ($name in @('install-common.ps1', 'windows-elevation.ps1', 'windows-prepared.ps1', 'payload-paths.ps1')) {
            Copy-Item -LiteralPath (Join-Path $repo "tools\scripts\lib\$name") -Destination $fakeLib
        }
        $fakeGenerated = Join-Path (Split-Path $fakeLib) 'generated'
        New-Item -ItemType Directory -Path $fakeGenerated | Out-Null
        Copy-Item -LiteralPath (Join-Path $repo 'tools\scripts\generated\manifest.windows.ps1') -Destination $fakeGenerated
        $shim = @'
. '__SERVICE__'
function Get-ScheduledTask { $null }
function Register-CoreServiceRelease { param($Release, $RepoRoot, $WorkingDirectory) Write-Output 'fixture register prepared' }
function Invoke-CoreServiceRelease { param($Release, $RepoRoot, $WorkingDirectory) Write-Output 'fixture guarded handoff' }
'@
        $shim.Replace('__SERVICE__', (Join-Path $repo 'tools\scripts\lib\windows-service.ps1').Replace("'", "''")) |
            Set-Content -LiteralPath (Join-Path $fakeLib 'windows-service.ps1')
        Set-Content -LiteralPath (Join-Path $fakeLib 'win-modules.ps1') -Value "throw 'Unexpected provisioning/build module load'"
        $profile = Join-Path $scratch 'resume profile'
        $root = Join-Path $profile '.continuum'
        New-Item -ItemType Directory -Path $profile | Out-Null
        Copy-Item -LiteralPath $resumeRoot -Destination $root -Recurse -Force
        $selected = $release | ConvertTo-Json | ConvertFrom-Json
        foreach ($field in @('artifact', 'cli', 'launcher', 'engine', 'logDirectory')) { $selected.$field = $selected.$field.Replace($resumeRoot, $root) }
        Save-CorePreparedRelease -Release $selected -InstallRoot $root
        foreach ($extra in @('', ' -Update')) {
            $info = [Diagnostics.ProcessStartInfo]::new((Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'))
            # Hidden PS5 ConsoleHost can omit terminating errors from redirected
            # stderr. Capture the exception explicitly without accepting failure.
            $entry = (Join-Path $fakeRepo 'install.ps1').Replace("'", "''")
            $invoke = "try { & '$entry' -ResumePrepared$extra } catch { Write-Output `$_.Exception.Message; exit 1 }"
            $info.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -Command "' + $invoke + '"'
            $info.UseShellExecute = $false
            $info.CreateNoWindow = $true
            $info.RedirectStandardOutput = $true
            $info.RedirectStandardError = $true
            $info.EnvironmentVariables['USERPROFILE'] = $profile
            $child = [Diagnostics.Process]::Start($info)
            try {
                $stdout = $child.StandardOutput.ReadToEndAsync()
                $stderr = $child.StandardError.ReadToEndAsync()
                # A bound sized for a LOADED runner, not an idle one: a child PowerShell's startup plus the
                # resume script blew a 10 s bound on 2026-09-20 (run 35523394865, a docs-only PR) and read
                # as a red tip. 120 s is the test's patience; a real hang still fails, named.
                if (-not $child.WaitForExit(120000)) { $child.Kill(); $child.WaitForExit(); throw 'Isolated resume installer fixture timed out (120 s)' }
                $output = $stdout.Result + $stderr.Result
                if (-not $extra) {
                    if ($child.ExitCode -ne 0 -or $output -notmatch 'fixture register prepared' -or $output -notmatch 'fixture guarded handoff') {
                        throw "Public prepared resume did not reach guarded handoff: $output"
                    }
                } elseif ($child.ExitCode -eq 0 -or $output -notmatch 'cannot be combined with -Update' -or $output -match 'fixture register prepared') {
                    throw "Public resume source-update refusal failed (exit $($child.ExitCode)): $output"
                }
            } finally { $child.Dispose() }
        }
    }
    Write-Output 'PASS: explicit prepared release validates integrity/identity/layout and retains guarded handoff ordering'

    # Regression for 6026ae26: Task Scheduler returned inherited administrator /
    # SYSTEM ACEs before an explicit caller-read ACE. AddAccess throws for this
    # real shape; repair must preserve every other ACE and its evaluation order.
    $callerSid = 'S-1-5-21-1-2-3-1004'
    $acl = "D:(A;ID;0x1f019f;;;BA)(A;ID;0x1f019f;;;SY)(A;ID;FA;;;BA)(A;;FR;;;$callerSid)"
    $old = [Security.AccessControl.CommonSecurityDescriptor]::new($false, $false, $acl)
    if ($old.IsDiscretionaryAclCanonical) { throw 'Regression did not reproduce the scheduler ACL shape' }
    $oldFailed = $false
    try {
        $old.DiscretionaryAcl.AddAccess([Security.AccessControl.AccessControlType]::Allow,
            [Security.Principal.SecurityIdentifier]::new($callerSid), 0x1200a9,
            [Security.AccessControl.InheritanceFlags]::None, [Security.AccessControl.PropagationFlags]::None)
    } catch { $oldFailed = $true }
    if (-not $oldFailed) { throw 'Old AddAccess unexpectedly accepted the noncanonical ACL' }
    $repaired = Grant-CoreServiceCallerAccess -Sddl $acl -UserSid $callerSid
    $before = [Security.AccessControl.RawSecurityDescriptor]::new($acl)
    $after = [Security.AccessControl.RawSecurityDescriptor]::new($repaired)
    if ($after.DiscretionaryAcl.Count -ne $before.DiscretionaryAcl.Count) { throw 'Repair replaced/added unexpected ACEs' }
    for ($i = 0; $i -lt $before.DiscretionaryAcl.Count; $i++) {
        $expected = $before.DiscretionaryAcl[$i]
        if ($i -eq 3) { $expected.AccessMask = $expected.AccessMask -bor 0x1200a9 }
        if (-not $expected.Equals($after.DiscretionaryAcl[$i])) { throw "Repair changed ACE $i beyond the caller mask" }
    }
    if (-not (Test-CoreServiceCallerAccess -Sddl $repaired -UserSid $callerSid) -or
        (Grant-CoreServiceCallerAccess -Sddl $repaired -UserSid $callerSid) -ne $repaired) { throw 'Caller grant failed verification/idempotence' }
    $withoutCaller = 'D:(A;ID;FA;;;BA)(A;ID;FA;;;SY)'
    $appended = [Security.AccessControl.RawSecurityDescriptor]::new(
        (Grant-CoreServiceCallerAccess -Sddl $withoutCaller -UserSid $callerSid))
    $original = [Security.AccessControl.RawSecurityDescriptor]::new($withoutCaller)
    if ($appended.DiscretionaryAcl.Count -ne 3 -or $appended.DiscretionaryAcl[2].AccessMask -ne 0x1200a9 -or
        $appended.DiscretionaryAcl[2].SecurityIdentifier.Value -ne $callerSid -or
        -not $original.DiscretionaryAcl[0].Equals($appended.DiscretionaryAcl[0]) -or
        -not $original.DiscretionaryAcl[1].Equals($appended.DiscretionaryAcl[1])) { throw 'Missing-caller repair did not append only the narrow grant' }
    foreach ($unsupported in @("D:(D;;GX;;;WD)(A;;FRFX;;;$callerSid)",
        "D:(D;;0x20;;;BA)(A;;FRFX;;;$callerSid)",
        'D:(XA;;FR;;;WD;(@User.department == "Finance"))',
        'D:(OA;;FR;11111111-1111-1111-1111-111111111111;;WD)')) {
        $original = [Security.AccessControl.RawSecurityDescriptor]::new($unsupported).GetSddlForm('Access')
        foreach ($operation in @('Grant-CoreServiceCallerAccess', 'Test-CoreServiceCallerAccess')) {
            $refused = $false
            try { & $operation -Sddl $unsupported -UserSid $callerSid | Out-Null }
            catch { $refused = $_ -match 'Unsupported startup task ACL' }
            if (-not $refused -or [Security.AccessControl.RawSecurityDescriptor]::new($unsupported).GetSddlForm('Access') -ne $original) {
                throw "Unsupported/denied ACL was accepted or changed by $operation : $unsupported"
            }
        }
    }
    Write-Output 'PASS: noncanonical task ACL is repaired minimally; denied/conditional/object policy stays untouched'
    # Repeated release updates require write, but never Delete/WriteDAC/WriteOwner.
    $updateAcl = Grant-CoreServiceCallerAccess -Sddl $repaired -UserSid $callerSid -Update
    if ((Test-CoreServiceCallerAccess -Sddl $repaired -UserSid $callerSid -Update) -or
        -not (Test-CoreServiceCallerAccess -Sddl $updateAcl -UserSid $callerSid -Update)) {
        throw 'Read/execute must not masquerade as update authority'
    }
    $updateAce = ([Security.AccessControl.RawSecurityDescriptor]::new($updateAcl)).DiscretionaryAcl[3]
    if (($updateAce.AccessMask -band 0xD0000) -ne 0) { throw 'Update grant acquired delete or ACL/owner privileges' }
    # Run the real registrar with only scheduler boundaries replaced. A provider
    # that ignores SetSecurityDescriptor must fail its reread, never claim success.
    & {
        $script:aclTask = [pscustomobject]@{ Sddl = $acl; Save = $true; Xml = 'original task XML' }
        $script:aclTask | Add-Member ScriptMethod GetSecurityDescriptor { param($flags) $this.Sddl }
        $script:aclTask | Add-Member ScriptMethod SetSecurityDescriptor { param($value, $flags) if ($this.Save) { $this.Sddl = $value } }
        $folder = [pscustomobject]@{ Restored = @() }
        $folder | Add-Member ScriptMethod RegisterTask { param($name,$xml,$flags,$sid,$password,$logon,$sddl)
            if ($xml -cne 'original task XML' -or $flags -ne 6 -or $logon -ne 2) { throw 'Rollback changed task contract' }
            $this.Restored += $name }
        $script:aclDeployTask = $null
        $folder | Add-Member ScriptMethod GetTask { param($name) if ($name -eq 'ContinuumDeploy' -and $script:aclDeployTask) { $script:aclDeployTask } else { $script:aclTask } }
        $script:aclScheduler = [pscustomobject]@{ Folder = $folder }
        $script:aclScheduler | Add-Member ScriptMethod Connect { }
        $script:aclScheduler | Add-Member ScriptMethod GetFolder { param($path) $this.Folder }
        function New-Object { param($ComObject) if ($ComObject -ne 'Schedule.Service') { throw 'Unexpected fixture COM request' }; $script:aclScheduler }
        function Get-ScheduledTask { [pscustomobject]@{} }
        function New-ScheduledTaskAction { [pscustomobject]@{} }
        function New-ScheduledTaskPrincipal { [pscustomobject]@{} }
        function New-ScheduledTaskTrigger { [pscustomobject]@{} }
        function New-ScheduledTaskSettingsSet { [pscustomobject]@{Enabled=$true} }
        $script:aclRegistrations = 0
        $script:failDeployProvision = $false
        function Register-ScheduledTask { param($TaskName)
            $script:aclRegistrations++
            if ($script:failDeployProvision -and $TaskName -eq 'ContinuumDeploy') { throw 'fixture second registration failed' } }
        $planPath = Join-Path $scratch 'acl-plan.json'
        @{ userSid = $callerSid; shell = 'fixture'; arguments = 'fixture'; description = 'fixture'; cli = 'fixture' } |
            ConvertTo-Json | Set-Content -LiteralPath $planPath -Encoding UTF8
        . (Join-Path $repo 'tools\scripts\register-core-service.ps1') -PlanPath $planPath
        # Two registrations: ContinuumCore, then the ContinuumDeploy consumer under the
        # same S4U principal — one registrar, one elevation, both tasks.
        if ($script:aclRegistrations -ne 2 -or -not (Test-CoreServiceCallerAccess -Sddl $script:aclTask.Sddl -UserSid $callerSid)) {
            throw 'Registrar did not register both tasks and persist/verify the caller grant'
        }
        $script:failDeployProvision = $true
        $refused = $false
        try { . (Join-Path $repo 'tools\scripts\register-core-service.ps1') -PlanPath $planPath } catch { $refused = $_ -match 'fixture second registration failed' }
        if (-not $refused -or $folder.Restored.Count -ne 2 -or 'ContinuumCore' -notin $folder.Restored -or 'ContinuumDeploy' -notin $folder.Restored) { throw 'Partial supervisor provisioning did not restore both prior tasks' }
        $script:failDeployProvision = $false
        $script:aclTask.Sddl = $acl
        $script:aclTask.Save = $false
        $refused = $false
        try { . (Join-Path $repo 'tools\scripts\register-core-service.ps1') -PlanPath $planPath }
        catch { $refused = $_ -match 'was not saved' }
        if (-not $refused) { throw 'Registrar reported success despite a missing saved grant' }
        $script:aclTask.Sddl = "D:(D;;GX;;;WD)(A;;FRFX;;;$callerSid)"
        $writes = $script:aclRegistrations
        $refused = $false
        try { . (Join-Path $repo 'tools\scripts\register-core-service.ps1') -PlanPath $planPath }
        catch { $refused = $_ -match 'Unsupported startup task ACL' }
        if (-not $refused -or $script:aclRegistrations -ne $writes) { throw 'Registrar changed task before refusing existing deny policy' }
        $script:aclDeployTask = [pscustomobject]@{ Sddl = $script:aclTask.Sddl }
        $script:aclDeployTask | Add-Member ScriptMethod GetSecurityDescriptor { param($flags) $this.Sddl }
        $script:aclTask.Sddl = $acl
        $refused = $false
        try { . (Join-Path $repo 'tools\scripts\register-core-service.ps1') -PlanPath $planPath }
        catch { $refused = $_ -match 'Unsupported startup task ACL' }
        if (-not $refused -or $script:aclRegistrations -ne $writes) { throw 'Registrar changed Core before refusing Deploy deny policy' }

    }
    Write-Output 'PASS: registrar rereads saved access and refuses unsupported policy before task writes'

    # what this catches: caller-local status must not shadow real native cache
    # probe/acquire/cleanup results in the shared artifact consumed by AIRC.
    & {
        . (Join-Path $repo 'tools\scripts\lib\windows-elevation.ps1')
        $nativeHelper = Join-Path $scratch 'native-status-fixture.exe'
        Add-Type -OutputAssembly $nativeHelper -OutputType ConsoleApplication -TypeDefinition @'
using System;
public static class NativeStatusFixture {
  public static int Main(string[] args) {
    if (args.Length > 0 && args[0] == "status") { Console.WriteLine("false"); return 1; }
    return 0;
  }
}
'@
        function Find-GsudoExecutable { $nativeHelper }
        function Test-IsAdmin { $false }
        $previousContext = $env:CAMBRIAN_INSTALL_ELEVATION
        $env:CAMBRIAN_INSTALL_ELEVATION = $null
        $LASTEXITCODE = 73
        try {
            Initialize-ElevationSession
            if ($script:InstallElevationSession.ExistingCache) { throw 'False native cache probe was masked' }
            Ensure-Elevated -Reason 'native status fixture'
            if (-not $script:ElevationWarmed) { throw 'Native successful acquisition was shadowed' }
            Clear-Elevation
            if ($env:CAMBRIAN_INSTALL_ELEVATION) { throw 'Native cleanup was shadowed' }
        } finally { $env:CAMBRIAN_INSTALL_ELEVATION = $previousContext }
        Write-Output 'PASS: real shared helper ignores caller-local stale native status'
    }
    # Regression for e1b774b1: a native elevation failure after a successful
    # build must retain its evidence and caller phase, not invent a UAC refusal.
    # Child scope confines mocks/preferences; cmd.exe supplies real stderr/exit.
    & {
        . (Join-Path $repo 'tools\scripts\lib\install-common.ps1')
        $script:ElevationWarmed = $false
        $script:elevationCalls = 0
        $script:elevationMode = 'failure'
        function Test-IsAdmin { $false }
        function Ensure-Gsudo { $script:GsudoExecutable = 'gsudo' }
        function Find-GsudoExecutable { 'gsudo' }
        function Test-ElevationCacheAvailable { $false }
        $script:gsudoArguments = @()
        function Invoke-InstallerProcess {
            param($FilePath, $ArgumentList, [switch]$OwnProcessTree, [switch]$PreserveChildrenOnSuccess)
            if ($FilePath -eq 'gsudo') { gsudo @ArgumentList }
            else { & $nativeInstallerProcess $FilePath $ArgumentList -OwnProcessTree:$OwnProcessTree -PreserveChildrenOnSuccess:$PreserveChildrenOnSuccess }
        }
        function gsudo {
            $script:elevationCalls++
            $script:gsudoArguments += ($args -join ' ')
            if ($script:elevationMode -eq 'failure') {
                & $nativeInstallerProcess $env:ComSpec -RawArguments '/d /c echo cache fixture stdout & echo cache fixture stderr 1>&2 & exit /b 73'
            } elseif ($script:elevationMode -eq 'empty') {
                & $nativeInstallerProcess $env:ComSpec -RawArguments '/d /c exit /b 74'
            } elseif ($script:elevationMode -eq 'cleanup-info') {
                & $nativeInstallerProcess $env:ComSpec -RawArguments '/d /c echo Info: Cache session closed. 1>&2 & exit /b 0'
            } else { $global:LASTEXITCODE = 0 }
        }
        $reason = 'registering the ContinuumCore startup task (before core handoff)'
        $failure = $null
        try { Invoke-Elevated -Reason $reason -CommandLine @('must-not-run') }
        catch { $failure = $_.Exception.Message }
        foreach ($expected in @($reason, 'exit 73', 'cache fixture stdout', 'cache fixture stderr')) {
            if (-not $failure -or -not $failure.Contains($expected)) { throw "Elevation failure lost evidence: $expected" }
        }
        if ($failure -match 'declined|VS Build Tools|CUDA' -or $script:ElevationWarmed -or
            $script:elevationCalls -ne 1 -or $ErrorActionPreference -ne 'Stop') {
            throw 'Failed elevation misdiagnosed cause, warmed cache, ran command, or changed caller preference'
        }
        $script:elevationMode = 'empty'
        $failure = $null
        try { Ensure-Elevated -Reason $reason } catch { $failure = $_.Exception.Message }
        if (-not $failure -or $failure -notmatch 'exit 74' -or $failure -notmatch 'no diagnostic output') {
            throw 'Missing elevation evidence was not explicit'
        }
        # No real consent: exercise receipts with a missing metadata path and
        # preserve the actual exit/launch failure rather than guessing an actor.
        $savedLauncher = ${function:Invoke-InstallerProcess}
        $script:consentReceipt = @()
        function Write-Host { param($Object) $script:consentReceipt += [string]$Object }
        try {
            function Invoke-InstallerProcess { param($FilePath, $ArgumentList) 'original consent diagnostic'; $global:LASTEXITCODE = 999 }
            $failure = $null
            try { Ensure-Elevated -Reason $reason } catch { $failure = $_.Exception.Message }
            if ($failure -notmatch 'exit 999' -or $failure -notmatch 'original consent diagnostic' -or
                $failure -notmatch 'does not establish which actor' -or $script:ElevationWarmed) { throw 'Consent status lost original evidence or inferred cancellation actor' }
            $receipt = $script:consentReceipt -join "`n"
            foreach ($expected in @('start: utc=', 'end: utc=', 'elapsedMs=', 'exit=999', 'gsudo=gsudo', 'fileVersion=unavailable', "ownerPid=$PID", "callerPid=$PID")) {
                if (-not $receipt.Contains($expected)) { throw "Consent receipt missing $expected" }
            }
            function Invoke-InstallerProcess { param($FilePath, $ArgumentList) throw 'original launch failure' }
            $failure = $null
            try { Ensure-Elevated -Reason $reason } catch { $failure = $_.Exception.Message }
            if ($failure -ne 'original launch failure' -or ($script:consentReceipt -join "`n") -notmatch 'launch-or-wait-failed') { throw 'Launch failure replaced by receipt diagnostics' }
            function Invoke-InstallerProcess { param($FilePath, $ArgumentList) $global:LASTEXITCODE = 0 }
            Ensure-Elevated -Reason $reason
            if (-not $script:ElevationWarmed -or ($script:consentReceipt -join "`n") -notmatch 'exit=0') { throw 'Successful consent receipt lost status' }
            $script:ElevationWarmed = $false
        } finally {
            Set-Item Function:Invoke-InstallerProcess $savedLauncher
            Remove-Item Function:Write-Host
        }
        $script:elevationMode = 'success'
        Ensure-Elevated -Reason $reason
        Ensure-Elevated -Reason $reason
        if (-not $script:ElevationWarmed -or $script:elevationCalls -ne 3) { throw 'Successful elevation was not cached exactly once' }
        # Regression: PS5 must not abort a completed registration on gsudo's
        # informational stderr when cache teardown actually succeeds.
        $script:elevationMode = 'cleanup-info'
        Clear-Elevation
        if ($script:ElevationWarmed -or $ErrorActionPreference -ne 'Stop') { throw 'Successful cleanup retained cache state or changed error policy' }
        Initialize-ElevationSession
        $script:ElevationWarmed = $true
        $script:elevationMode = 'failure'
        $failure = $null
        try { Clear-Elevation } catch { $failure = $_.Exception.Message }
        if (-not $failure -or $failure -notmatch 'exit 73' -or -not $script:ElevationWarmed) { throw 'Failed cache cleanup was silently accepted' }
        $script:elevationCalls = 3
        $script:ElevationWarmed = $false
        function Test-IsAdmin { $true }
        Ensure-Elevated -Reason $reason
        if (-not $script:ElevationWarmed -or $script:elevationCalls -ne 3) { throw 'Already elevated path invoked gsudo' }
        Clear-Elevation

        # Shared-installer regression: a child can be the first admin caller,
        # but only the outer owner disposes its process-scoped cache. No global
        # authorization, no default five-minute expiry during a build.
        function Test-IsAdmin { $false }
        $script:elevationMode = 'success'
        Initialize-ElevationSession
        $parentSession = $script:InstallElevationSession
        $parentContext = $env:CAMBRIAN_INSTALL_ELEVATION
        # Real process boundaries: same-PID mocks cannot exercise ancestry.
        $helperPath = (Join-Path $repo 'tools\scripts\lib\windows-elevation.ps1').Replace("'", "''")
        $probe = @"
`$ErrorActionPreference = 'Stop'
try {
    . '$helperPath'
    Initialize-ElevationSession
    if (-not `$script:InstallElevationSession.Borrowed -or `$script:InstallElevationSession.OwnerPid -ne $PID) { throw 'Child did not borrow expected owner.' }
    Clear-Elevation
    if (-not `$env:CAMBRIAN_INSTALL_ELEVATION) { throw 'Child removed parent context.' }
    Write-Output 'fixture process borrowed and released locally'
} catch { Write-Output `$_.Exception.Message; exit 1 }
"@
        $encodedProbe = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($probe))
        $powerShellExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
        $bashExe = Get-Command git.exe -CommandType Application -All -ErrorAction Stop | ForEach-Object {
            Join-Path (Split-Path (Split-Path $_.Source)) 'bin\bash.exe'
        } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if (-not $bashExe) { throw 'Git Bash is required for the Windows installer ancestry regression.' }
        foreach ($viaBash in @($false, $true)) {
            $probeInfo = [Diagnostics.ProcessStartInfo]::new($powerShellExe)
            $probeInfo.Arguments = "-NoProfile -NonInteractive -EncodedCommand $encodedProbe"
            if ($viaBash) {
                $probeInfo.FileName = $bashExe
                $probeInfo.Arguments = '--noprofile --norc -c "powershell.exe -NoProfile -NonInteractive -EncodedCommand ' + $encodedProbe + '"'
            }
            $probeInfo.UseShellExecute = $false
            $probeInfo.CreateNoWindow = $true
            $probeInfo.RedirectStandardOutput = $true
            $probeInfo.RedirectStandardError = $true
            $probeChild = [Diagnostics.Process]::Start($probeInfo)
            try {
                $probeOut = $probeChild.StandardOutput.ReadToEndAsync()
                $probeErr = $probeChild.StandardError.ReadToEndAsync()
                if (-not $probeChild.WaitForExit(120000)) { $probeChild.Kill(); $probeChild.WaitForExit(); throw 'Elevation ancestry child timed out' }
                $probeOutput = $probeOut.Result + $probeErr.Result
                if ($probeChild.ExitCode -ne 0 -or $probeOutput -notmatch 'fixture process borrowed and released locally') {
                    throw "Elevation ancestry failed (Git Bash=$viaBash): $probeOutput"
                }
            } finally { $probeChild.Dispose() }
        }
        $script:InstallElevationSession = $null
        Initialize-ElevationSession
        if (-not $script:InstallElevationSession.Borrowed) { throw 'Nested installer claimed parent cache ownership' }
        Ensure-Elevated -Reason 'child firewall fixture'
        if ($script:gsudoArguments[-1] -ne "cache on -p $PID -d -1") { throw 'Cache was not bound to the installer lifetime/process' }
        $callsBeforeChildCleanup = $script:elevationCalls
        Clear-Elevation
        if ($script:elevationCalls -ne $callsBeforeChildCleanup -or $env:CAMBRIAN_INSTALL_ELEVATION -ne $parentContext) {
            throw 'Borrowed cleanup disposed or hid the outer owner context'
        }
        $script:InstallElevationSession = $parentSession
        $script:ElevationWarmed = $false
        Clear-Elevation
        if ($script:elevationCalls -ne ($callsBeforeChildCleanup + 1) -or
            $script:gsudoArguments[-1] -ne "cache off -p $PID" -or $env:CAMBRIAN_INSTALL_ELEVATION) {
            throw 'Outer owner failed to close a child-acquired cache'
        }
        # A no-work install in an interactive shell must not clear its existing
        # caller-owned cache, nor extend that cache when it borrows elevation.
        function Test-ElevationCacheAvailable { $true }
        $callsBeforeExisting = $script:elevationCalls
        Initialize-ElevationSession
        if (-not $script:InstallElevationSession.ExistingCache) { throw 'Pre-existing cache was claimed by installer' }
        Clear-Elevation
        Initialize-ElevationSession
        Ensure-Elevated -Reason 'borrowing an existing cache'
        if ($script:elevationCalls -ne $callsBeforeExisting) { throw 'Existing cache was reacquired or cleared by installer' }
        function Test-ElevationCacheAvailable { $false }
        $failure = $null
        try { Ensure-Elevated -Reason 'expired borrowed cache' } catch { $failure = $_.Exception.Message }
        if (-not $failure -or $failure -notmatch 'pre-existing elevation cache expired') { throw 'Expired external cache silently reacquired consent' }
        Clear-Elevation
        if ($script:elevationCalls -ne $callsBeforeExisting) { throw 'External cache cleanup invoked gsudo' }
        $invalid = $parentContext | ConvertFrom-Json
        $invalid.ownerStarted = '0'
        $env:CAMBRIAN_INSTALL_ELEVATION = $invalid | ConvertTo-Json -Compress
        $failure = $null
        try { Initialize-ElevationSession } catch { $failure = $_.Exception.Message }
        Remove-Item Env:CAMBRIAN_INSTALL_ELEVATION
        if (-not $failure -or $failure -notmatch 'owner process has changed' -or $script:InstallElevationSession) {
            throw 'Stale inherited elevation owner was accepted'
        }
    }
    Write-Output 'PASS: elevation failure preserves native diagnostics and phase without guessing cause'

    # A nested install can register gsudo without updating its parent's PATH.
    # Use a real scratch executable; the registry boundary alone is synthetic.
    & {
        . (Join-Path $repo 'tools\scripts\lib\windows-elevation.ps1')
        $registeredBin = Join-Path $scratch 'registered-gsudo'
        New-Item -ItemType Directory $registeredBin | Out-Null
        $registeredExe = Join-Path $registeredBin 'gsudo.exe'
        Add-Type -OutputAssembly $registeredExe -OutputType ConsoleApplication -TypeDefinition @'
using System;
public static class RegisteredGsudoFixture {
    public static int Main(string[] args) {
        if (args.Length != 1 || args[0] != "--version") return 91;
        var version = Environment.GetEnvironmentVariable("GSUDO_FIXTURE_VERSION") ?? "gsudo v2.6.1";
        if (version == "nonzero") { Console.WriteLine("gsudo v2.6.1 failed probe"); return 17; }
        Console.WriteLine(version);
        return 0;
    }
}
'@
        $savedPath = $env:PATH
        $savedVersion = $env:GSUDO_FIXTURE_VERSION
        $script:registeredRefreshes = 0
        function Update-SessionPath { $script:registeredRefreshes++; $env:PATH = $registeredBin }
        try {
            $env:PATH = Join-Path $scratch 'empty-path'
            Ensure-Gsudo
            if ($script:GsudoExecutable -ne $registeredExe -or $script:registeredRefreshes -ne 1) {
                throw 'Registered native helper was not reused from stale caller PATH'
            }
            # No winget exists in either fixture PATH: a redundant acquisition
            # fails this test. Repeated discovery must execute/verify the tool.
            Ensure-Gsudo
            if ($script:registeredRefreshes -ne 1) { throw 'Registered helper reuse unnecessarily refreshed PATH' }
            $env:GSUDO_FIXTURE_VERSION = 'nonzero'
            $failure = $null
            try { Ensure-Gsudo } catch { $failure = $_.Exception.Message }
            if ($failure -notmatch 'exit 17' -or $failure -notmatch 'failed probe') {
                throw 'Failed native version probe was accepted or its diagnostic lost'
            }
            $env:GSUDO_FIXTURE_VERSION = 'unexpected executable'
            $failure = $null
            try { Ensure-Gsudo } catch { $failure = $_.Exception.Message }
            if ($failure -notmatch 'failed version verification' -or $failure -notmatch 'unexpected executable') {
                throw 'Unverified registered executable was accepted or its diagnostic lost'
            }
        } finally { $env:PATH = $savedPath; $env:GSUDO_FIXTURE_VERSION = $savedVersion }
    }
    Write-Output 'PASS: stale caller PATH reuses the registered executable with native version proof'

    # The shared helper must consume manifest data, including in standalone
    # consumers. Missing/unsupported source data must never start acquisition.
    & {
        . (Join-Path $repo 'tools\scripts\lib\windows-elevation.ps1') -GsudoSource @{type='winget';id='fixture.package';scope='user'}
        $script:gsudoFinds = 0
        $script:gsudoPackageArgs = @()
        function Find-GsudoExecutable { $script:gsudoFinds++; if ($script:gsudoFinds -gt 1) { 'fixture-native.exe' } }
        function Update-SessionPath { }
        function winget { $script:gsudoPackageArgs = @($args); $global:LASTEXITCODE = 0 }
        function Invoke-InstallerProcess {
            param($FilePath, $ArgumentList, [switch]$OwnProcessTree, [switch]$PreserveChildrenOnSuccess)
            if ($FilePath -eq 'winget') { winget @ArgumentList }
            else { & $nativeInstallerProcess $FilePath $ArgumentList -OwnProcessTree:$OwnProcessTree -PreserveChildrenOnSuccess:$PreserveChildrenOnSuccess }
        }
        Ensure-Gsudo
        if ($script:GsudoExecutable -ne 'fixture-native.exe' -or
            ($script:gsudoPackageArgs -join ' ') -notmatch '--id fixture.package --source winget' -or
            ($script:gsudoPackageArgs -join ' ') -notmatch '--scope user') { throw 'gsudo acquisition ignored manifest source' }
        $script:ElevationGsudoSource = @{type='winget';id='fixture.package';scope='machine'}
        $script:gsudoFinds = 0
        $script:gsudoPackageArgs = @()
        $failure = $null
        try { Ensure-Gsudo } catch { $failure = $_.Exception.Message }
        if (-not $failure -or $failure -notmatch 'per-user gsudo package source' -or $script:gsudoPackageArgs.Count) {
            throw 'Unsupported elevation-helper acquisition was attempted'
        }
    }
    Write-Output 'PASS: standalone elevation acquisition uses the shared manifest and rejects unsupported scope'

    $installed = Join-Path $scratch 'installed with spaces'
    $installedPayload = Initialize-ManagedPayloadRoot -HomeRoot $installed -ColdRoot (Join-Path $scratch 'cold service payloads')
    $target = Join-Path $scratch 'cargo'
    New-Item -ItemType Directory -Path (Join-Path $target 'release') | Out-Null
    foreach ($name in @('continuum.exe', 'continuum-core-server.exe', 'livekit-bridge.exe')) {
        Set-Content -LiteralPath (Join-Path $target "release\$name") -Value 'candidate'
    }
    # CI rollback failed to load (0xc0000135) when installer staging omitted its declared CUDA DLLs.
    Set-Content -LiteralPath (Join-Path $target 'release\runtime-libs.txt') -Value 'fixture-runtime.dll'
    Set-Content -LiteralPath (Join-Path $target 'release\fixture-runtime.dll') -Value 'runtime candidate'
    $script:liveProcesses = @()
    $script:registeredTask = $null
    function Get-CimInstance { param($ClassName, $ErrorAction) $script:liveProcesses }
    function Get-ScheduledTask { param($TaskName, $TaskPath, $ErrorAction) $script:registeredTask }
    $first = New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target
    # Prebuilt handoff bypasses start-server: media must travel with the slot.
    $mediaSlot = Split-Path -Parent $first.artifact
    if ((Get-FileHash -LiteralPath (Join-Path $mediaSlot 'fixture-runtime.dll')).Hash -ne
        (Get-FileHash -LiteralPath (Join-Path $target 'release\fixture-runtime.dll')).Hash) { throw 'Declared CI runtime DLL was not staged intact' }
    Set-Content -LiteralPath (Join-Path $target 'release\runtime-libs.txt') -Value '../escape.dll'
    $badRuntimeRefused = $false
    try { New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target | Out-Null } catch { $badRuntimeRefused = $_ -match 'Invalid or missing declared core runtime library' }
    if (-not $badRuntimeRefused) { throw 'Runtime library path traversal was accepted' }
    Set-Content -LiteralPath (Join-Path $target 'release\runtime-libs.txt') -Value 'absent-runtime.dll'
    $missingRuntimeRefused = $false
    try { New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target | Out-Null } catch { $missingRuntimeRefused = $_ -match 'Invalid or missing declared core runtime library' }
    if (-not $missingRuntimeRefused) { throw 'Missing declared runtime library was accepted' }
    Set-Content -LiteralPath (Join-Path $target 'release\runtime-libs.txt') -Value 'fixture-runtime.dll'
    foreach ($media in @('livekit-bridge.exe', 'start-livekit-windows.ps1')) {
        if (-not (Test-Path -LiteralPath (Join-Path $mediaSlot $media))) { throw "Missing staged media artifact: $media" }
    }
    if ($first.artifact -ne (Join-Path $installedPayload 'bin\service-a\continuum-core-server.exe')) { throw 'Empty install did not select first slot' }
    $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum-core-server.exe'; ExecutablePath = $first.artifact })
    $second = New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target
    if ($first.artifact -eq $second.artifact) { throw 'Overwrote a live slot' }
    $script:liveProcesses[0].ExecutablePath = '\\?\' + $first.artifact
    $extended = New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target
    if ($extended.artifact -ne $second.artifact) { throw 'Extended Windows process path was not recognized as a live slot' }
    if ((ConvertTo-CoreImagePath '\\?\UNC\server\share\core.exe') -ne '\\server\share\core.exe') { throw 'Extended UNC image path normalization failed' }
    $script:registeredTask = [pscustomobject]@{ Description = ($first | ConvertTo-Json -Compress) }
    $script:liveProcesses = @()
    $stopped = New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target
    if ($stopped.artifact -ne $second.artifact) { throw 'Overwrote registered release while stopped' }
    $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum-core-server.exe'; ExecutablePath = $first.artifact })
    $script:liveProcesses += [pscustomobject]@{ Name = 'llama-server.exe'; ExecutablePath = (Join-Path (Split-Path $second.artifact) 'llama-server.exe') }
    $withEngine = New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target
    if ($withEngine.artifact -ne $second.artifact) { throw 'An adopted engine blocked reuse of its core slot' }
    $savedProcesses = $script:liveProcesses
    $registeredRelease = $first | ConvertTo-Json | ConvertFrom-Json
    $registeredRelease.engine = '\\?\' + (Join-Path $installedPayload 'bin\engine-b\llama-server.exe')
    $script:registeredTask = [pscustomobject]@{ Description = ($registeredRelease | ConvertTo-Json -Compress) }
    $script:liveProcesses = @([pscustomobject]@{ Name = 'llama-server.exe'; ExecutablePath = ('\\?\' + (Join-Path $installedPayload 'bin\engine-a\llama-server.exe')) })
    $candidate = New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target
    if ($candidate.engine -ne (Join-Path $installedPayload 'bin\engine-c\llama-server.exe')) { throw 'Candidate overwrote a warm or registered engine' }
    $script:registeredTask = [pscustomobject]@{ Description = ($first | ConvertTo-Json -Compress) }
    $script:liveProcesses = $savedProcesses
    $script:liveProcesses += [pscustomobject]@{ Name = 'continuum.exe'; ExecutablePath = $second.cli }
    $refused = $false
    try { New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target | Out-Null } catch { $refused = $_ -match 'Both installed core service slots' }
    if (-not $refused) { throw 'Two live slots were not protected' }
    $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum.exe'; ExecutablePath = $null })
    $unreadable = New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target
    if ($unreadable.artifact -ne $second.artifact) { throw 'Hidden image lost registered-slot protection' }
    $busy = [IO.File]::Open($second.artifact, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::None)
    try {
        $refused = $false
        try { New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target | Out-Null } catch { $refused = $_ -match 'Both installed core service slots' }
        if (-not $refused) { throw 'Unreadable busy candidate was overwritten' }
    } finally { $busy.Dispose() }
    $script:registeredTask = $null
    $refused = $false
    try { New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target | Out-Null } catch { $refused = $_ -match 'Cannot inspect all live' }
    if (-not $refused) { throw 'Inaccessible image path was treated as an empty slot' }
    Write-Output 'PASS: active core/engine slots and inaccessible image paths are protected'

    # Card 2c5d0ec0: with a CLI that knows `continuum engine idle-slot`, the engine slot is the
    # CORE's answer from its lane records, not a process-table guess (a service-session lane's
    # path is unreadable from here). Exit 3 refuses; an answer outside the slots refuses; a
    # readable live engine inside the answer refuses.
    $fakeCli = Join-Path $scratch 'fake-continuum-cli.ps1'
    function Invoke-InstallerProcess {
        param($FilePath, $ArgumentList, [switch]$OwnProcessTree, [switch]$PreserveChildrenOnSuccess)
        if ($FilePath -eq $fakeCli) { & $FilePath @ArgumentList }
        else { & $nativeInstallerProcess $FilePath $ArgumentList -OwnProcessTree:$OwnProcessTree -PreserveChildrenOnSuccess:$PreserveChildrenOnSuccess }
    }
    Set-Content -LiteralPath $fakeCli -Value @'
if ($args[0] -eq '--help') { 'continuum engine idle-slot'; 'continuum engine promote <slot> <commit:backend>'; exit 0 }
$payload = Get-ManagedPayloadRoot -HomeRoot $env:CONTINUUM_HOME
if ($args[0] -eq 'engine' -and $args[1] -eq 'promote') {
    if ($env:FAKE_PROMOTE_RC) { 'refused'; exit ([int]$env:FAKE_PROMOTE_RC) }
    Set-Content -LiteralPath (Join-Path $payload 'bin\current') -Value $args[2]
    Set-Content -LiteralPath (Join-Path $env:CONTINUUM_HOME 'promoted-with') -Value "$($args[2]) $($args[3])"
    exit 0
}
if ($args[0] -eq 'engine' -and $args[1] -eq 'idle-slot') {
    if ($env:FAKE_IDLE_RC) { exit ([int]$env:FAKE_IDLE_RC) }
    Join-Path $payload ('bin\' + $env:FAKE_IDLE_SLOT); exit 0
}
exit 64
'@
    $script:liveProcesses = @([pscustomobject]@{ Name = 'llama-server.exe'; ExecutablePath = $null })
    try {
        $env:FAKE_IDLE_SLOT = 'engine-b'; $env:FAKE_IDLE_RC = $null
        $picked = Select-CoreEngineSlot -InstallRoot $installed -Descriptor $null -Cli $fakeCli
        if ($picked -ne (ConvertTo-CoreImagePath (Join-Path $installedPayload 'bin\engine-b'))) { throw "The core's idle slot was not used: $picked" }
        $env:FAKE_IDLE_RC = '3'
        $refused = $false
        try { Select-CoreEngineSlot -InstallRoot $installed -Descriptor $null -Cli $fakeCli | Out-Null } catch { $refused = $_ -match 'All installed engine slots' }
        if (-not $refused) { throw 'No idle slot (exit 3) was not refused' }
        # card 3f8f5754: a DEPLOY that meets every slot busy skips the engine, as bash does.
        if ($null -ne (Select-CoreEngineSlot -InstallRoot $installed -Descriptor $null -Cli $fakeCli -SkipIfBusy)) {
            throw 'A deploy with every slot busy did not skip the engine'
        }
        $env:FAKE_IDLE_RC = $null; $env:FAKE_IDLE_SLOT = 'service-a'
        $refused = $false
        try { Select-CoreEngineSlot -InstallRoot $installed -Descriptor $null -Cli $fakeCli | Out-Null } catch { $refused = $_ -match 'not an engine slot' }
        if (-not $refused) { throw 'An answer outside the engine slots was accepted' }
        $env:FAKE_IDLE_SLOT = 'engine-a'
        $script:liveProcesses = @([pscustomobject]@{ Name = 'llama-server.exe'; ExecutablePath = (Join-Path $installedPayload 'bin\engine-a\llama-server.exe') })
        $refused = $false
        try { Select-CoreEngineSlot -InstallRoot $installed -Descriptor $null -Cli $fakeCli | Out-Null } catch { $refused = $_ -match 'running engine executes from it' }
        if (-not $refused) { throw 'A readable live engine inside the core answer was overwritten' }
    } finally { $env:FAKE_IDLE_SLOT = $null; $env:FAKE_IDLE_RC = $null; $script:liveProcesses = @() }
    # The pre-verb path skips a busy deploy too: every slot live by the process table.
    $script:liveProcesses = @('engine-a', 'engine-b', 'engine-c' | ForEach-Object {
        [pscustomobject]@{ Name = 'llama-server.exe'; ExecutablePath = (Join-Path $installedPayload "bin\$_\llama-server.exe") } })
    try {
        if ($null -ne (Select-CoreEngineSlot -InstallRoot $installed -Descriptor $null -SkipIfBusy)) { throw 'The pre-verb path did not skip a busy deploy' }
        $refused = $false
        try { Select-CoreEngineSlot -InstallRoot $installed -Descriptor $null | Out-Null } catch { $refused = $_ -match 'All installed engine slots' }
        if (-not $refused) { throw 'A first install with every slot live was not refused' }
    } finally { $script:liveProcesses = @() }
    Write-Output 'PASS: the engine slot is the core answer from its lane records when the CLI knows the verb'

    # card d5584dfc: a drift-verified slot is promoted by the core's own verb (current is the one
    # truth), with the stamp the build wrote; a refused promote throws; a CLI without the verb
    # leaves the release registration to bootstrap, and says so.
    $promoteSlot = Join-Path $installedPayload 'bin\engine-c'
    New-Item -ItemType Directory -Force -Path $promoteSlot | Out-Null
    Set-Content -LiteralPath (Join-Path $promoteSlot '.llama-server.stamp') -Value 'abc1234:cuda'
    try {
        if (-not (Invoke-CoreEnginePromote -Cli $fakeCli -InstallRoot $installed -Slot $promoteSlot)) { throw 'A CLI with the verb did not promote' }
        if ((Get-Content -LiteralPath (Join-Path $installed 'promoted-with') -Raw).Trim() -cne 'engine-c abc1234:cuda') { throw 'Promoted without the slot stamp' }
        $env:FAKE_PROMOTE_RC = '1'
        $refused = $false
        try { Invoke-CoreEnginePromote -Cli $fakeCli -InstallRoot $installed -Slot $promoteSlot | Out-Null } catch { $refused = $_ -match 'refused engine-c' }
        if (-not $refused) { throw 'A refused promote was not surfaced' }
        $env:FAKE_PROMOTE_RC = $null
        if (Invoke-CoreEnginePromote -Cli (Join-Path $scratch 'no-such-cli.exe') -InstallRoot $installed -Slot $promoteSlot 3>$null) { throw 'A missing CLI claimed a promotion' }
    } finally { $env:FAKE_PROMOTE_RC = $null }
    Write-Output 'PASS: a verified engine slot is promoted by the core verb with its own stamp'

    # card 6d5bacab (Codex on #4512): Prepare-CoreServiceEngine promotes a non-current slot that
    # ALREADY holds the pinned engine, with no idle-slot question and no build; a drifting slot,
    # a promote that cannot run, and a release that changes mid-way each take their own road.
    # Isolated scope: every collaborator is mocked, so this proves the decision, not the build.
    & {
        $profileRoot = Join-Path $scratch 'already-built-profile'
        $payload = Initialize-ManagedPayloadRoot -HomeRoot (Join-Path $profileRoot '.continuum') -ColdRoot (Join-Path $scratch 'engine preparation cold')
        $bin = Join-Path $payload 'bin'
        foreach ($name in @('engine-a', 'engine-b', 'engine-c')) {
            New-Item -ItemType Directory -Force -Path (Join-Path $bin $name) | Out-Null
            Set-Content -LiteralPath (Join-Path $bin "$name\llama-server.exe") -Value 'engine'
        }
        $script:releaseJson = (@{ cli = (Join-Path $scratch 'fake-cli.exe') } | ConvertTo-Json -Compress)
        $script:matching = 'engine-c'
        $script:promoteResult = $true
        $script:selected = $false
        $script:changeTaskAfter = $false
        $script:taskReads = 0
        function Get-ScheduledTask {
            $script:taskReads++
            if ($script:changeTaskAfter -and $script:taskReads -gt 1) { return [pscustomobject]@{ Description = '{"changed":true}' } }
            [pscustomobject]@{ Description = $script:releaseJson }
        }
        function Get-CoreEngineRequirement { [pscustomobject]@{ source_revision = ('a' * 40); backend = 'cuda' } }
        function Get-CoreEngineDrift { param($Directory, $Requirement) if ((Split-Path -Leaf $Directory) -eq $script:matching) { '' } else { 'drift' } }
        function Invoke-CoreEnginePromote { param($Cli, $InstallRoot, $Slot) $script:promoted = Split-Path -Leaf $Slot; $script:promoteResult }
        function Select-CoreEngineSlot { $script:selected = $true; $null }
        function Mod-LlamaServer { throw 'fixture: the already-built path must not build' }
        $savedProfile = $env:USERPROFILE
        $receipt = Join-Path $scratch 'already-built-receipt'
        try {
            $env:USERPROFILE = $profileRoot
            # (1) engine-c already at the pin: promoted, no idle-slot question, no build
            Prepare-CoreServiceEngine -RepoRoot $scratch -Description $script:releaseJson -ReceiptPath $receipt
            if ($script:promoted -ne 'engine-c' -or $script:selected) { throw 'A slot already at the pin was not promoted directly' }
            if ([IO.File]::ReadAllText($receipt) -ne (Join-Path $bin 'engine-c\llama-server.exe')) { throw 'The receipt does not name the promoted slot' }
            # (2) no slot at the pin: falls through to idle-slot selection (a busy set skips)
            $script:matching = 'none'; $script:selected = $false; $script:promoted = $null
            Prepare-CoreServiceEngine -RepoRoot $scratch -Description $script:releaseJson -ReceiptPath $receipt
            if (-not $script:selected -or $script:promoted) { throw 'A drifting slot was promoted instead of falling through' }
            if (-not ([IO.File]::ReadAllText($receipt)).StartsWith('SKIP: ')) { throw 'The fall-through did not reach the idle-slot path' }
            # (3) at the pin but the promote cannot run (a CLI without the verb): falls through
            $script:matching = 'engine-c'; $script:promoteResult = $false; $script:selected = $false
            Prepare-CoreServiceEngine -RepoRoot $scratch -Description $script:releaseJson -ReceiptPath $receipt
            if (-not $script:selected) { throw 'A promote that could not run did not fall through' }
            # (4) the release changes during preparation: refused, nothing handed off
            $script:promoteResult = $true; $script:changeTaskAfter = $true; $script:taskReads = 0
            $refused = $false
            try { Prepare-CoreServiceEngine -RepoRoot $scratch -Description $script:releaseJson -ReceiptPath $receipt } catch { $refused = $_ -match 'changed during engine preparation' }
            if (-not $refused) { throw 'A release changed mid-preparation was handed off' }
        } finally { $env:USERPROFILE = $savedProfile }
    }
    Write-Output 'PASS: a slot already at the pin is promoted without a build, and every other case takes its own road'

    # Compile a tiny native child: arguments containing spaces must
    # arrive unchanged and a nonzero exit must reach Task Scheduler.
    $child = Join-Path $scratch 'child with spaces.exe'
    Add-Type -TypeDefinition @'
using System;
public class SupervisorFixture {
    public static int Main(string[] args) {
        if (args.Length == 1 && args[0] == "--version") {
            var name = System.IO.Path.GetFileName(System.Reflection.Assembly.GetExecutingAssembly().Location);
            Console.WriteLine(name == "nvcc.exe" ? "Cuda compilation tools, release 99.0" : "cmake version 99.0.0");
            return 0;
        }
        if (args.Length == 4 && args[0] == "reboot" && args[1] == "--prebuilt" && args[3] == "--validate-only") {
            Console.WriteLine("fixture prebuilt validated");
            return 0;
        }
        if (args.Length == 4 && args[1] == "fork") {
            var info = new System.Diagnostics.ProcessStartInfo(
                System.Reflection.Assembly.GetExecutingAssembly().Location,
                "hold \"" + args[2] + "\"");
            info.UseShellExecute = false;
            info.CreateNoWindow = true;
            info.RedirectStandardOutput = true;
            info.RedirectStandardError = true;
            var child = System.Diagnostics.Process.Start(info);
            System.IO.File.WriteAllText(args[3], child.Id.ToString());
            return 7;
        }
        if (args.Length == 2 && args[0] == "hold") {
            System.IO.File.WriteAllText(args[1], "ready");
            System.Threading.Thread.Sleep(10000);
            return 0;
        }
        Console.WriteLine(string.Join("|", args));
        Console.Error.WriteLine("child failure receipt");
        return 7;
    }
}
'@ -OutputAssembly $child -OutputType ConsoleApplication
    # Regression for 9e3818b9: run the actual installer, slot allocator, CLI
    # preflight and receipt writer, mocking only build/scheduler boundaries.
    & {
        $prepareRepo = Join-Path $scratch 'prepare installer'
        $prepareLib = Join-Path $prepareRepo 'tools\scripts\lib'
        $prepareProfile = Join-Path $scratch 'prepare profile'
        $prepareRoot = Join-Path $prepareProfile '.continuum'
        $oldSlot = Join-Path $prepareRoot 'bin\service-a'
        New-Item -ItemType Directory -Path $prepareLib, $oldSlot -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $repo 'install.ps1') -Destination $prepareRepo
        foreach ($name in @('install-common.ps1', 'windows-elevation.ps1', 'windows-prepared.ps1', 'payload-paths.ps1')) {
            Copy-Item -LiteralPath (Join-Path $repo "tools\scripts\lib\$name") -Destination $prepareLib
        }
        $prepareGenerated = Join-Path (Split-Path $prepareLib) 'generated'
        New-Item -ItemType Directory -Path $prepareGenerated | Out-Null
        Copy-Item -LiteralPath (Join-Path $repo 'tools\scripts\generated\manifest.windows.ps1') -Destination $prepareGenerated
        Copy-Item -LiteralPath (Join-Path $repo 'tools\scripts\run-service-hidden.ps1') -Destination (Split-Path $prepareLib)
        Copy-Item -LiteralPath (Join-Path $repo 'tools\scripts\start-livekit-windows.ps1') -Destination (Split-Path $prepareLib)
        $oldArtifact = Join-Path $oldSlot 'continuum-core-server.exe'
        Set-Content -LiteralPath $oldArtifact -Value 'registered candidate must survive'
        $oldHash = (Get-FileHash -LiteralPath $oldArtifact).Hash
        $cmakeBin = Join-Path $prepareRoot 'tools\cmake\bin'
        $llvmBin = Join-Path $prepareRoot 'tools\llvm\bin'
        $cudaBin = Join-Path $prepareRoot 'cuda-toolkit\bin'
        New-Item -ItemType Directory -Path $cmakeBin, $llvmBin, $cudaBin -Force | Out-Null
        Copy-Item -LiteralPath $child -Destination (Join-Path $cmakeBin 'cmake.exe')
        Copy-Item -LiteralPath $child -Destination (Join-Path $cudaBin 'nvcc.exe')
        Set-Content -LiteralPath (Join-Path $llvmBin 'libclang.dll') -Value 'fixture'
        & {
            . (Join-Path $repo 'tools\scripts\lib\win-modules.ps1')
            $llvmRoot = Split-Path $llvmBin
            $source = (Get-ManifestModule 'llvm-libclang').source
            foreach ($relative in @(Get-LlvmRequiredPaths $source)) {
                $file = Join-Path $llvmRoot $relative
                New-Item -ItemType Directory -Path (Split-Path $file) -Force | Out-Null
                [IO.File]::WriteAllText($file, 'fixture')
            }
            $receipt = Get-LlvmStagedReceipt -Directory $llvmRoot -Source $source
            [IO.File]::WriteAllText((Join-Path $llvmRoot 'llvm-install.json'), ($receipt | ConvertTo-Json -Depth 5))
        }
        $shim = @'
. '__SERVICE__'
function Get-CimInstance { @() }
function Get-ScheduledTask {
    [pscustomobject]@{ Description = (@{artifact=(Join-Path $env:USERPROFILE '.continuum\bin\service-a\continuum-core-server.exe'); engine=(Join-Path $env:USERPROFILE '.continuum\bin\engine-a\llama-server.exe')} | ConvertTo-Json) }
}
function Invoke-CoreServiceRelease { throw 'Unexpected live handoff' }
function Invoke-Elevated { throw 'Unexpected elevation' }
function Ensure-Elevated { throw 'Unexpected elevation' }
function Test-WingetAvailable { throw 'Unexpected provisioning' }
function git { $global:LASTEXITCODE = 0 }
$fixtureNativeProcess = ${function:Invoke-InstallerProcess}
function Invoke-InstallerProcess {
    param($FilePath, $ArgumentList, [switch]$OwnProcessTree, [switch]$PreserveChildrenOnSuccess)
    if ($FilePath -eq 'git') { $global:LASTEXITCODE = 0 }
    else { & $fixtureNativeProcess $FilePath $ArgumentList -OwnProcessTree:$OwnProcessTree -PreserveChildrenOnSuccess:$PreserveChildrenOnSuccess }
}
'@
        $shim.Replace('__SERVICE__', (Join-Path $repo 'tools\scripts\lib\windows-service.ps1').Replace("'", "''")) |
            Set-Content -LiteralPath (Join-Path $prepareLib 'windows-service.ps1')
        $modules = @'
. '__MODULES__'
function Get-Command { param($Name) if ($Name -ne 'cmake') { [pscustomobject]@{Source=$Name} } }
function Invoke-WebRequest { throw 'Unexpected download' }
function Invoke-RestMethod { throw 'Unexpected download' }
function Mod-BuildCore {
    if (-not $env:CMAKE -or -not $env:LIBCLANG_PATH -or -not $env:CUDA_PATH) { throw 'Cached toolchain environment was not restored' }
    $env:CARGO_TARGET_DIR = Join-Path $env:USERPROFILE 'fixture-target'
    $release = Join-Path $env:CARGO_TARGET_DIR 'release'
    New-Item -ItemType Directory -Path $release -Force | Out-Null
    foreach ($name in @('continuum.exe','continuum-core-server.exe','livekit-bridge.exe')) { Copy-Item -LiteralPath $env:CONTINUUM_FIXTURE_CHILD -Destination (Join-Path $release $name) }
}
function Mod-LlamaServer {
    param($RepoRoot,$InstallDirectory)
    New-Item -ItemType Directory -Path $InstallDirectory -Force | Out-Null
    Copy-Item -LiteralPath $env:CONTINUUM_FIXTURE_CHILD -Destination (Join-Path $InstallDirectory 'llama-server.exe')
}
'@
        $modules.Replace('__MODULES__', (Join-Path $repo 'tools\scripts\lib\win-modules.ps1').Replace("'", "''")) |
            Set-Content -LiteralPath (Join-Path $prepareLib 'win-modules.ps1')
        Set-Content -LiteralPath (Join-Path $prepareLib 'windows-prebuilt.ps1') -Value "function Get-CorePrebuiltRelease { throw 'fixture reached published fetch without developer tools' }"
        $missingFiles = @{cmake=(Join-Path $cmakeBin 'cmake.exe'); llvm=(Join-Path $llvmBin 'libclang.dll'); cuda=(Join-Path $cudaBin 'nvcc.exe')}
        foreach ($extra in @('', ' -Update', ' -Grid', ' -ResumePrepared', 'cmake', 'llvm', 'cuda', 'prebuilt')) {
            $missing = $missingFiles[$extra]
            if ($missing) { $missingBytes = [IO.File]::ReadAllBytes($missing); Remove-Item -LiteralPath $missing }
            $info = [Diagnostics.ProcessStartInfo]::new((Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'))
            # Same explicit exception capture as the hidden resume fixture.
            $entry = (Join-Path $prepareRepo 'install.ps1').Replace("'", "''")
            $flags = if ($missing -or $extra -eq 'prebuilt') { '' } else { $extra }
            $developerFlag = if ($extra -eq 'prebuilt') { '' } else { ' -DeveloperBuild' }
            $invoke = "try { & '$entry' -PrepareOnly$developerFlag$flags } catch { Write-Output `$_.Exception.Message; exit 1 }"
            $info.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -Command "' + $invoke + '"'
            $info.UseShellExecute = $false
            $info.CreateNoWindow = $true
            $info.RedirectStandardOutput = $true
            $info.RedirectStandardError = $true
            $info.EnvironmentVariables['USERPROFILE'] = $prepareProfile
            $info.EnvironmentVariables['CONTINUUM_FIXTURE_CHILD'] = $child
            foreach ($variable in @('CMAKE', 'LIBCLANG_PATH', 'CUDA_PATH')) { $info.EnvironmentVariables.Remove($variable) }
            $process = [Diagnostics.Process]::Start($info)
            try {
                $stdout = $process.StandardOutput.ReadToEndAsync()
                $stderr = $process.StandardError.ReadToEndAsync()
                if (-not $process.WaitForExit(120000)) { $process.Kill(); $process.WaitForExit(); throw 'Isolated prepare fixture timed out (120 s)' }
                $output = $stdout.Result + $stderr.Result
                if ($extra -eq 'prebuilt') {
                    if ($process.ExitCode -eq 0 -or $output -notmatch 'fixture reached published fetch without developer tools') { throw "Default preparation did not select published artifacts: $output" }
                } elseif (-not $extra) {
                    if ($process.ExitCode -ne 0 -or $output -notmatch 'fixture prebuilt validated') { throw "Public preparation failed: $output" }
                } elseif ($missing) {
                    if ($process.ExitCode -eq 0 -or $output -notmatch 'Preparation requires' -or $output -match 'Unexpected download') { throw "Missing cached toolchain did not fail before provisioning: $output" }
                } elseif ($process.ExitCode -eq 0 -or $output -notmatch 'cannot be combined') { throw "Preparation flag refusal failed (exit $($process.ExitCode)): $output" }
            } finally {
                $process.Dispose()
                if ($missing) { [IO.File]::WriteAllBytes($missing, $missingBytes) }
            }
        }
        if ((Get-FileHash -LiteralPath $oldArtifact).Hash -ne $oldHash) { throw 'Preparation overwrote the registered candidate' }
        function Write-Step { param($msg) }
        function Get-ScheduledTask { $null }
        $prepared = Get-CorePreparedRelease -InstallRoot $prepareRoot
        if ($prepared.artifact -ne (Join-Path $prepareRoot 'bin\service-b\continuum-core-server.exe') -or
            $prepared.engine -ne (Join-Path $prepareRoot 'bin\engine-b\llama-server.exe')) { throw 'Preparation selected a registered slot' }
        $script:resumedArtifact = $null
        function Register-CoreServiceRelease { param($Release,$RepoRoot,$WorkingDirectory) $script:resumedArtifact = $Release.artifact }
        function Invoke-CoreServiceRelease { param($Release,$RepoRoot,$WorkingDirectory) if ($Release.artifact -ne $script:resumedArtifact) { throw 'Resume changed prepared candidate' } }
        Resume-CorePreparedRelease -RepoRoot $repo -InstallRoot $prepareRoot
        if ($script:resumedArtifact -ne $prepared.artifact) { throw 'Resume did not consume the new preparation receipt' }
    }
    Write-Output 'PASS: public prepare stages/validates/resumes without provisioning, elevation, handoff, or registered-slot overwrite'
    $logs = Join-Path $scratch 'logs'
    New-Item -ItemType Directory -Path $logs | Out-Null
    $shell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $runner = Join-Path $scratch 'run-service-hidden.ps1'
    Copy-Item -LiteralPath (Join-Path $repo 'tools\scripts\run-service-hidden.ps1') -Destination $runner
    Copy-Item -LiteralPath $child -Destination (Join-Path $scratch 'livekit-bridge.exe')
    Set-Content -LiteralPath (Join-Path $scratch 'start-livekit-windows.ps1') -Value 'param([string]$BridgeBinary); Set-Content -LiteralPath (Join-Path $PSScriptRoot "media-started") -Value $BridgeBinary'
    $core = Join-Path $scratch 'core with spaces.exe'
    $socket = Join-Path $scratch 'socket with spaces.sock'
    & $shell -NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File $runner -ExecutablePath $child -CorePath $core -SocketPath $socket -EnginePath $child -LogDirectory $logs
    if ($LASTEXITCODE -ne 7) { throw "Hidden host lost child exit code: $LASTEXITCODE" }
    if ((Get-Content -LiteralPath (Join-Path $scratch 'media-started') -Raw).Trim() -ne (Join-Path $scratch 'livekit-bridge.exe')) { throw 'Prebuilt supervisor did not start its staged media bundle' }
    if ((Get-Content (Join-Path $logs 'service.out.log') -Raw).Trim() -ne "service-host|$core|$socket|$child") { throw 'Hidden host changed argument boundaries' }
    if ((Get-Content (Join-Path $logs 'service.err.log') -Raw).Trim() -ne 'child failure receipt') { throw 'Hidden host lost stderr' }
    Write-Output 'PASS: native supervisor preserves arguments, both logs, and child exit 7'

    $childMarker = Join-Path $scratch 'descendant-ready'
    $childPidFile = Join-Path $scratch 'descendant-pid'
    $elapsed = [Diagnostics.Stopwatch]::StartNew()
    try {
        & $shell -NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File $runner -ExecutablePath $child -CorePath fork -SocketPath $childMarker -EnginePath $childPidFile -LogDirectory $logs
        if ($LASTEXITCODE -ne 7 -or $elapsed.Elapsed.TotalSeconds -ge 5) { throw 'Supervisor waited for surviving descendant or lost host exit code' }
        $descendant = Get-Process -Id ([int](Get-Content $childPidFile)) -ErrorAction Stop
        if ($descendant.HasExited) { throw 'Supervisor killed its surviving inference-shaped child' }
    } finally {
        if ($descendant -and -not $descendant.HasExited) { $descendant.Kill(); $descendant.WaitForExit(); $descendant.Dispose() }
    }
    Write-Output 'PASS: supervisor reports host exit while a warm descendant remains alive'

    # Published engines use an explicit backend on GPU-less CI and cannot reuse
    # a native-CPU stamp. Extend this fixture without building an engine.
    & {
        . (Join-Path $repo 'tools\scripts\lib\win-modules.ps1')
        $engineRepo = Join-Path $scratch 'publisher-repo'
        $engineOut = Join-Path $scratch 'publisher-engine'
        New-Item -ItemType Directory -Force (Join-Path $engineRepo 'core\vendor\llama.cpp\tools\server'), $engineOut | Out-Null
        [IO.File]::WriteAllText((Join-Path $engineRepo 'core\vendor\llama.cpp\tools\server\CMakeLists.txt'), '# fixture')
        [IO.File]::WriteAllText((Join-Path $engineOut 'llama-server.exe'), 'fixture engine')
        Save-CoreEngineReceipt -Directory $engineOut -SourceRevision ('a' * 40) -Backend cuda
        function Get-CoreEngineBackend { throw 'Publisher probed the build host GPU.' }
        function Set-CudaTargets { throw 'Publisher queried native GPU targets.' }
        function Get-ManagedPayloadRoot { return (Join-Path $scratch 'publisher-payload') }
        function Invoke-InstallerProcess {
            param($FilePath, $ArgumentList, [switch]$OwnProcessTree)
            if ($FilePath -ne 'git') { throw 'Publisher reuse unexpectedly ran a build tool.' }
            $global:LASTEXITCODE = 0
            if ($ArgumentList -contains '--short') { return 'aaaaaaa' }
            return ('a' * 40)
        }
        function Module-Skip { }
        function Module-Start { }
        function Mod-CMake { throw 'fixture: fresh published build required' }
        $stamp = Join-Path $engineOut '.llama-server.stamp'
        [IO.File]::WriteAllText($stamp, 'aaaaaaa:cuda:80:portable-v1')
        Mod-LlamaServer -RepoRoot $engineRepo -InstallDirectory $engineOut -RequireReceipt -PublishedCudaArchitectures 80 -PublishedCpuDefinitions @('-DGGML_NATIVE=OFF')
        [IO.File]::WriteAllText($stamp, 'aaaaaaa:cuda:80')
        $refused = $false
        try { Mod-LlamaServer -RepoRoot $engineRepo -InstallDirectory $engineOut -RequireReceipt -PublishedCudaArchitectures 80 -PublishedCpuDefinitions @('-DGGML_NATIVE=OFF') }
        catch { $refused = $_ -match 'fresh published build required' }
        if (-not $refused) { throw 'Publisher reused a native engine stamp.' }
        foreach ($invalid in @(
            @{ PublishedCudaArchitectures = '80'; PublishedCpuDefinitions = @('-DGGML_NATIVE=OFF') },
            @{ PublishedCudaArchitectures = '80'; RequireReceipt = $true },
            @{ PublishedCudaArchitectures = '80'; RequireReceipt = $true; PublishedCpuDefinitions = @('-DGGML_NATIVE=ON') }
        )) {
            $refused = $false
            try { Mod-LlamaServer -RepoRoot $engineRepo -InstallDirectory $engineOut @invalid }
            catch { $refused = $_ -match 'Published engine requires' }
            if (-not $refused) { throw 'Incomplete publisher contract was accepted.' }
        }
    }
    # Exercise the actual CMake import resolver with a deterministic inspector
    # adapter: only the platform driver may be absent on a GPU-less build host.
    $cmake = Get-Command cmake -ErrorAction SilentlyContinue
    $cmakePath = if ($cmake) { $cmake.Source } else { Join-Path $env:USERPROFILE '.continuum\tools\cmake\bin\cmake.exe' }
    if (-not (Test-Path -LiteralPath $cmakePath)) { throw 'CMake is required for the engine import fixture.' }
    $imports = Join-Path $scratch 'engine-imports'
    New-Item -ItemType Directory -Path $imports | Out-Null
    [IO.File]::WriteAllText((Join-Path $imports 'llama-server.exe'), 'inspector fixture')
    $inspector = Join-Path $imports 'inspect.cmd'
    $inspection = "@echo off`r`necho Dump of file fixture`r`necho File Type: EXECUTABLE IMAGE`r`necho   Image has the following dependencies:`r`necho.`r`necho     nvcuda.dll`r`necho.`r`necho   Summary`r`n"
    [IO.File]::WriteAllText($inspector, $inspection, [Text.Encoding]::ASCII)
    $importArgs = @("-DCMAKE_GET_RUNTIME_DEPENDENCIES_COMMAND=$inspector", "-DENGINE_DIR=$($imports.Replace('\','/'))", "-DSYSTEM_DIR=$([Environment]::SystemDirectory.Replace('\','/'))", '-P', (Join-Path $repo 'tools/scripts/lib/verify-engine-imports.cmake'))
    & $cmakePath @importArgs
    if ($LASTEXITCODE -ne 0) { throw 'Absent platform driver was not accepted on the build host.' }
    $shadow = Join-Path $imports 'NvCuDa.dll'
    [IO.File]::WriteAllText($shadow, 'forbidden shadow')
    $errorLog = Join-Path $imports 'refusal.log'
    $savedPreference = $ErrorActionPreference
    try { $ErrorActionPreference = 'Continue'; & $cmakePath @importArgs 2> $errorLog }
    finally { $ErrorActionPreference = $savedPreference }
    if ($LASTEXITCODE -eq 0 -or (Get-Content $errorLog -Raw) -notmatch 'must not shadow') { throw 'Application driver shadow was accepted.' }
    Remove-Item -LiteralPath $shadow
    [IO.File]::WriteAllText($inspector, $inspection.Replace('nvcuda.dll', 'unowned-engine-runtime.dll'), [Text.Encoding]::ASCII)
    try { $ErrorActionPreference = 'Continue'; & $cmakePath @importArgs 2> $errorLog }
    finally { $ErrorActionPreference = $savedPreference }
    if ($LASTEXITCODE -eq 0 -or (Get-Content $errorLog -Raw) -notmatch 'Unresolved/conflicting engine imports') { throw 'An unrelated unresolved import was accepted.' }
    # A developer's System32 OpenMP installation is not an OS dependency. The
    # same resolver must report it for the CLI bootstrap and engine packaging.
    $platform = Join-Path $scratch 'runtime-platform'
    New-Item -ItemType Directory -Path $platform | Out-Null
    [IO.File]::WriteAllText((Join-Path $platform 'vcomp140.dll'), 'platform-installed redist')
    [IO.File]::WriteAllText($inspector, $inspection.Replace('nvcuda.dll', 'vcomp140.dll'), [Text.Encoding]::ASCII)
    $runtimeArgs = @("-DCMAKE_GET_RUNTIME_DEPENDENCIES_COMMAND=$inspector", "-DENGINE_DIR=$($imports.Replace('\','/'))", "-DSYSTEM_DIR=$([Environment]::SystemDirectory.Replace('\','/'))", "-DRUNTIME_DIRS=$($platform.Replace('\','/'))", '-P', (Join-Path $repo 'tools/scripts/lib/verify-engine-imports.cmake'))
    try { $ErrorActionPreference = 'Continue'; & $cmakePath @runtimeArgs 2> $errorLog }
    finally { $ErrorActionPreference = $savedPreference }
    if ($LASTEXITCODE -eq 0 -or (Get-Content $errorLog -Raw) -notmatch 'Redistributable must be bundled|Engine imports outside app/platform roots') { throw 'Unstaged redist was incorrectly treated as Windows.' }
    Copy-Item -LiteralPath (Join-Path $platform 'vcomp140.dll') -Destination $imports
    $bootstrapNames = Join-Path $imports 'bootstrap.txt'
    $captureArgs = @("-DCMAKE_GET_RUNTIME_DEPENDENCIES_COMMAND=$inspector", "-DENGINE_DIR=$($imports.Replace('\','/'))", "-DSYSTEM_DIR=$([Environment]::SystemDirectory.Replace('\','/'))", '-DCAPTURE_IMPORTS=ON', "-DOUTPUT_NAMES=$bootstrapNames", '-P', (Join-Path $repo 'tools/scripts/lib/verify-engine-imports.cmake'))
    & $cmakePath @captureArgs
    if ($LASTEXITCODE -ne 0 -or @(Get-Content $bootstrapNames) -notcontains 'vcomp140.dll') { throw 'CLI runtime closure omitted application-local OpenMP.' }
    & {
        . (Join-Path $repo 'tools/scripts/lib/windows-runtime-closure.ps1')
        function Invoke-InstallerProcess {
            param($FilePath, $ArgumentList, [switch]$OwnProcessTree)
            if ($FilePath -ne 'cmake') { throw 'Unexpected runtime packaging adapter.' }
            & $cmakePath @ArgumentList
        }
        $freshRuntime = Join-Path $scratch 'fresh-runtime-package'
        New-Item -ItemType Directory -Path $freshRuntime | Out-Null
        $freshCli = Join-Path $freshRuntime 'continuum.exe'
        [IO.File]::WriteAllText($freshCli, 'fixture CLI')
        $freshNames = Join-Path $freshRuntime 'bootstrap-runtime-libs.txt'
        Copy-CoreRuntimeClosure -Directory $freshRuntime -Executables @($freshCli) -Inspector $inspector -RuntimeDirectories @($platform) -OutputNames $freshNames
        if (@(Get-Content $freshNames) -notcontains 'vcomp140.dll' -or
            (Get-FileHash (Join-Path $freshRuntime 'vcomp140.dll')).Hash -cne (Get-FileHash (Join-Path $platform 'vcomp140.dll')).Hash) {
            throw 'Runtime publisher did not preserve toolchain OpenMP bytes and bootstrap membership.'
        }
    }
    & {
        . (Join-Path $repo 'tools/scripts/lib/windows-runtime-closure.ps1')
        $openssl = Join-Path $scratch 'configured OpenSSL'
        $include = Join-Path $openssl 'include'
        $tlsBin = Join-Path $openssl 'bin'
        $redist = Join-Path $scratch 'selected-redist'
        New-Item -ItemType Directory -Force -Path $include,$tlsBin,(Join-Path $redist 'x64/Microsoft.VC999.CRT') | Out-Null
        foreach ($name in @('libssl-3-x64.dll','libcrypto-3-x64.dll')) {
            [IO.File]::WriteAllText((Join-Path $tlsBin $name), ('selected TLS '+$name))
        }
        # Regression: an external TLS DLL imports VC runtimes that also exist in System32.
        $vcNames = @(Get-Content (Join-Path $repo 'tools/scripts/lib/windows-runtime-redistributables.txt'))
        foreach ($name in $vcNames) { [IO.File]::WriteAllText((Join-Path $redist ('x64/Microsoft.VC999.CRT/'+$name)), ('selected VC '+$name)) }
        $savedRedist = $env:VCToolsRedistDir
        try {
            $env:VCToolsRedistDir = $redist
            $roots = @(Get-CoreRuntimeDirectories -CMakeCache ("OPENSSL_INCLUDE_DIR:PATH="+$include+"`n"))
            if ($roots -notcontains $tlsBin) { throw 'Configured TLS runtime directory was omitted.' }
            $tlsStage = Join-Path $scratch 'tls-stage'
            New-Item -ItemType Directory -Path $tlsStage | Out-Null
            $tlsExe = Join-Path $tlsStage 'llama-server.exe'
            [IO.File]::WriteAllText($tlsExe, 'TLS engine fixture')
            $tlsInspection = $inspection.Replace('nvcuda.dll', "libssl-3-x64.dll`r`necho     libcrypto-3-x64.dll")
            $vcInspection = $inspection.Replace('nvcuda.dll', ($vcNames -join "`r`necho     "))
            [IO.File]::WriteAllText($inspector, ("@echo off`r`nif /I `%~nx2`==libssl-3-x64.dll goto vc`r`nif /I `%~nx2`==libcrypto-3-x64.dll goto vc`r`n"+$tlsInspection+"exit /b 0`r`n:vc`r`n"+$vcInspection), [Text.Encoding]::ASCII)
            function Invoke-InstallerProcess { param($FilePath,$ArgumentList,[switch]$OwnProcessTree) & $cmakePath @ArgumentList }
            $tlsNames = Join-Path $tlsStage 'runtime-imports.txt'
            Copy-CoreRuntimeClosure -Directory $tlsStage -Executables @($tlsExe) -Inspector $inspector -RuntimeDirectories $roots -OutputNames $tlsNames
            foreach ($name in @('libssl-3-x64.dll','libcrypto-3-x64.dll')) {
                if (@(Get-Content $tlsNames) -notcontains $name -or (Get-FileHash (Join-Path $tlsStage $name)).Hash -cne (Get-FileHash (Join-Path $tlsBin $name)).Hash) { throw 'TLS runtime was not captured and hashed from the configured package.' }
            }
            foreach ($name in $vcNames) {
                if (@(Get-Content $tlsNames) -notcontains $name -or
                    (Get-FileHash (Join-Path $tlsStage $name)).Hash -cne (Get-FileHash (Join-Path $redist ('x64/Microsoft.VC999.CRT/'+$name))).Hash) {
                    throw 'Transitive VC runtime omitted or inherited from the developer machine.'
                }
            }
            # Force discovery through the external DLL again, without the PS
            # staging step repairing the deliberately damaged staged runtime.
            foreach ($name in @('libssl-3-x64.dll','libcrypto-3-x64.dll')) { Remove-Item -LiteralPath (Join-Path $tlsStage $name) }
            $damaged = Join-Path $tlsStage 'vcruntime140.dll'
            [IO.File]::WriteAllText($damaged, 'wrong runtime bytes')
            $tlsArgs = @("-DCMAKE_GET_RUNTIME_DEPENDENCIES_COMMAND=$inspector", "-DENGINE_DIR=$($tlsStage.Replace('\','/'))", "-DSYSTEM_DIR=$([Environment]::SystemDirectory.Replace('\','/'))", "-DRUNTIME_DIRS=$(($roots | ForEach-Object { $_.Replace('\','/') }) -join ';')", '-DCAPTURE_IMPORTS=ON', '-P', (Join-Path $repo 'tools/scripts/lib/verify-engine-imports.cmake'))
            try { $ErrorActionPreference = 'Continue'; & $cmakePath @tlsArgs 2> $errorLog }
            finally { $ErrorActionPreference = $savedPreference }
            if ($LASTEXITCODE -eq 0 -or (Get-Content $errorLog -Raw) -notmatch 'Staged redistributable differs') { throw 'Capture accepted a mismatched staged VC runtime.' }
            # Prior dependencies may have been copied before the refusal.
            foreach ($name in @('libssl-3-x64.dll','libcrypto-3-x64.dll')) { Remove-Item -LiteralPath (Join-Path $tlsStage $name) -ErrorAction SilentlyContinue }
            Remove-Item -LiteralPath $damaged
            try { $ErrorActionPreference = 'Continue'; & $cmakePath @tlsArgs 2> $errorLog }
            finally { $ErrorActionPreference = $savedPreference }
            if ($LASTEXITCODE -eq 0 -or (Get-Content $errorLog -Raw) -notmatch 'Redistributable must be bundled') { throw 'Capture inherited a missing VC runtime from Windows.' }
        } finally { $env:VCToolsRedistDir = $savedRedist }
    }
    Write-Output 'PASS: publisher preserves declared hardware contract and bounds engine imports on GPU-less hosts'

    $output = Join-Path $target 'release\continuum-core-server.exe'
    Copy-Item -LiteralPath $child -Destination $output -Force
    $marker = Join-Path $scratch 'ready'
    $held = Start-Process -FilePath $output -ArgumentList @('hold', ('"' + $marker + '"')) -WindowStyle Hidden -PassThru
    try {
        $until = [DateTime]::UtcNow.AddSeconds(5)
        while (-not (Test-Path $marker)) {
            if ($held.HasExited -or [DateTime]::UtcNow -ge $until) { throw 'Held child did not initialize' }
            Start-Sleep -Milliseconds 50
        }
        $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum-core-server.exe'; ExecutablePath = $null })
        $refused = $false
        try { Protect-CoreBuildOutput -TargetDirectory $target } catch { $refused = $_ -match 'Cannot inspect running core image paths' }
        if (-not $refused -or $held.HasExited -or -not (Test-Path $output)) { throw 'Unknown busy image was not preserved' }
        $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum-core-server.exe'; ExecutablePath = ('\\?\' + $output) })
        Protect-CoreBuildOutput -TargetDirectory $target
        if ($held.HasExited -or (Test-Path $output)) { throw 'Busy output was not preserved live under its previous name' }
        $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum-core-server.exe'; ExecutablePath = $null })
        Copy-Item -LiteralPath $child -Destination $output
        # A free Cargo output is safe even when the service hides its image path.
        Protect-CoreBuildOutput -TargetDirectory $target
        if (-not (Test-Path $output) -or $held.HasExited) { throw 'Hidden service path blocked writable output or disturbed live core' }
        $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum-core-server.exe'; ExecutablePath = ('\\?\' + $output) })
        # CIM retains the old path after rename: a retry must recognize that
        # the new output is writable instead of deleting the mapped old file.
        Protect-CoreBuildOutput -TargetDirectory $target
        if (-not (Test-Path $output) -or $held.HasExited) { throw 'Retry disturbed the running image or fresh output' }
    } finally {
        if (-not $held.HasExited) { $held.Kill(); $held.WaitForExit() }
        $held.Dispose()
    }
    Write-Output 'PASS: mapped Cargo image stays alive while its replacement is writable, including retry'
} finally {
    # Remove only this test's verified temporary directory, in one shell.
    $resolved = [IO.Path]::GetFullPath($scratch)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or
        (Split-Path $resolved -Leaf) -notlike 'continuum-service-test-*') { throw 'Unsafe test cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
# Expected child failures deliberately set LASTEXITCODE to 7. Report the test
# suite's success explicitly so a dot-sourcing CI shell does not inherit it.
exit 0
