# Regression for #4056: exercise installed-slot safety and the real hidden
# supervisor without registering tasks or changing the operator's environment.
# Run with Windows PowerShell 5.1 (also the scheduler's production runtime).
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $repo 'tools\scripts\lib\windows-service.ps1')
. (Join-Path $repo 'tools\scripts\lib\windows-prepared.ps1')
. (Join-Path $repo 'tools\scripts\lib\windows-engine-receipt.ps1')
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('continuum-service-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
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
. '$($repo.Replace("'", "''"))/tools/scripts/lib/windows-service.ps1'
. '$($repo.Replace("'", "''"))/tools/scripts/lib/win-modules.ps1'
`$drift=Get-CoreEngineDrift -Directory '$($engineFixture.Replace("'", "''"))' -Requirement ([pscustomobject]@{source_revision='$('a' * 40)';backend='cuda'})
if (`$drift) { throw `$drift }
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
    # Regression for 72920541: retrying registration must select exact prepared
    # files, never treat an unchecked descriptor as a source-build cache hit.
    & {
        function Write-Step { param($msg) }
        $resumeRoot = Join-Path $scratch 'resume installed'
        $serviceSlot = Join-Path $resumeRoot 'bin\service-a'
        $engineSlot = Join-Path $resumeRoot 'bin\engine-a'
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
        $redirect = Join-Path $resumeRoot 'bin\service-b'
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
        foreach ($name in @('install-common.ps1', 'windows-prepared.ps1')) {
            Copy-Item -LiteralPath (Join-Path $repo "tools\scripts\lib\$name") -Destination $fakeLib
        }
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
            $info.Arguments = '-NoProfile -ExecutionPolicy RemoteSigned -File "' + (Join-Path $fakeRepo 'install.ps1') + '" -ResumePrepared' + $extra
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
                    throw 'Public resume accepted source-update mode'
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

    # Run the real registrar with only scheduler boundaries replaced. A provider
    # that ignores SetSecurityDescriptor must fail its reread, never claim success.
    & {
        $script:aclTask = [pscustomobject]@{ Sddl = $acl; Save = $true }
        $script:aclTask | Add-Member ScriptMethod GetSecurityDescriptor { param($flags) $this.Sddl }
        $script:aclTask | Add-Member ScriptMethod SetSecurityDescriptor { param($value, $flags) if ($this.Save) { $this.Sddl = $value } }
        $folder = [pscustomobject]@{}
        $folder | Add-Member ScriptMethod GetTask { param($name) $script:aclTask }
        $script:aclScheduler = [pscustomobject]@{ Folder = $folder }
        $script:aclScheduler | Add-Member ScriptMethod Connect { }
        $script:aclScheduler | Add-Member ScriptMethod GetFolder { param($path) $this.Folder }
        function New-Object { param($ComObject) if ($ComObject -ne 'Schedule.Service') { throw 'Unexpected fixture COM request' }; $script:aclScheduler }
        function Get-ScheduledTask { [pscustomobject]@{} }
        function New-ScheduledTaskAction { [pscustomobject]@{} }
        function New-ScheduledTaskPrincipal { [pscustomobject]@{} }
        function New-ScheduledTaskTrigger { [pscustomobject]@{} }
        function New-ScheduledTaskSettingsSet { [pscustomobject]@{} }
        $script:aclRegistrations = 0
        function Register-ScheduledTask { $script:aclRegistrations++ }
        $planPath = Join-Path $scratch 'acl-plan.json'
        @{ userSid = $callerSid; shell = 'fixture'; arguments = 'fixture'; description = 'fixture'; cli = 'fixture' } |
            ConvertTo-Json | Set-Content -LiteralPath $planPath -Encoding UTF8
        . (Join-Path $repo 'tools\scripts\register-core-service.ps1') -PlanPath $planPath
        # Two registrations: ContinuumCore, then the ContinuumDeploy consumer under the
        # same S4U principal — one registrar, one elevation, both tasks.
        if ($script:aclRegistrations -ne 2 -or -not (Test-CoreServiceCallerAccess -Sddl $script:aclTask.Sddl -UserSid $callerSid)) {
            throw 'Registrar did not register both tasks and persist/verify the caller grant'
        }
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
    }
    Write-Output 'PASS: registrar rereads saved access and refuses unsupported policy before task writes'

    # Regression for e1b774b1: a native elevation failure after a successful
    # build must retain its evidence and caller phase, not invent a UAC refusal.
    # Child scope confines mocks/preferences; cmd.exe supplies real stderr/exit.
    & {
        . (Join-Path $repo 'tools\scripts\lib\install-common.ps1')
        $script:ElevationWarmed = $false
        $script:elevationCalls = 0
        $script:elevationMode = 'failure'
        function Test-IsAdmin { $false }
        function Ensure-Gsudo { }
        function gsudo {
            $script:elevationCalls++
            if ($script:elevationMode -eq 'failure') {
                & "$env:SystemRoot\System32\cmd.exe" /d /c 'echo cache fixture stdout & echo cache fixture stderr 1>&2 & exit /b 73'
            } elseif ($script:elevationMode -eq 'empty') {
                & "$env:SystemRoot\System32\cmd.exe" /d /c 'exit /b 74'
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
        $script:elevationMode = 'success'
        Ensure-Elevated -Reason $reason
        Ensure-Elevated -Reason $reason
        if (-not $script:ElevationWarmed -or $script:elevationCalls -ne 3) { throw 'Successful elevation was not cached exactly once' }
        $script:ElevationWarmed = $false
        function Test-IsAdmin { $true }
        Ensure-Elevated -Reason $reason
        if (-not $script:ElevationWarmed -or $script:elevationCalls -ne 3) { throw 'Already elevated path invoked gsudo' }
    }
    Write-Output 'PASS: elevation failure preserves native diagnostics and phase without guessing cause'

    $installed = Join-Path $scratch 'installed with spaces'
    $target = Join-Path $scratch 'cargo'
    New-Item -ItemType Directory -Path (Join-Path $target 'release') | Out-Null
    foreach ($name in @('continuum.exe', 'continuum-core-server.exe')) {
        Set-Content -LiteralPath (Join-Path $target "release\$name") -Value 'candidate'
    }
    $script:liveProcesses = @()
    $script:registeredTask = $null
    function Get-CimInstance { param($ClassName, $ErrorAction) $script:liveProcesses }
    function Get-ScheduledTask { param($TaskName, $TaskPath, $ErrorAction) $script:registeredTask }
    $first = New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target
    if ($first.artifact -ne (Join-Path $installed 'bin\service-a\continuum-core-server.exe')) { throw 'Empty install did not select first slot' }
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
    $registeredRelease.engine = '\\?\' + (Join-Path $installed 'bin\engine-b\llama-server.exe')
    $script:registeredTask = [pscustomobject]@{ Description = ($registeredRelease | ConvertTo-Json -Compress) }
    $script:liveProcesses = @([pscustomobject]@{ Name = 'llama-server.exe'; ExecutablePath = ('\\?\' + (Join-Path $installed 'bin\engine-a\llama-server.exe')) })
    $candidate = New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target
    if ($candidate.engine -ne (Join-Path $installed 'bin\engine-c\llama-server.exe')) { throw 'Candidate overwrote a warm or registered engine' }
    $script:registeredTask = [pscustomobject]@{ Description = ($first | ConvertTo-Json -Compress) }
    $script:liveProcesses = $savedProcesses
    $script:liveProcesses += [pscustomobject]@{ Name = 'continuum.exe'; ExecutablePath = $second.cli }
    $refused = $false
    try { New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target | Out-Null } catch { $refused = $_ -match 'Both installed core service slots' }
    if (-not $refused) { throw 'Two live slots were not protected' }
    $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum.exe'; ExecutablePath = $null })
    $refused = $false
    try { New-CoreServiceRelease -RepoRoot $repo -InstallRoot $installed -TargetDirectory $target | Out-Null } catch { $refused = $_ -match 'Cannot inspect all live' }
    if (-not $refused) { throw 'Inaccessible image path was treated as an empty slot' }
    Write-Output 'PASS: active core/engine slots and inaccessible image paths are protected'

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
        foreach ($name in @('install-common.ps1', 'windows-prepared.ps1')) {
            Copy-Item -LiteralPath (Join-Path $repo "tools\scripts\lib\$name") -Destination $prepareLib
        }
        Copy-Item -LiteralPath (Join-Path $repo 'tools\scripts\run-service-hidden.ps1') -Destination (Split-Path $prepareLib)
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
    foreach ($name in @('continuum.exe','continuum-core-server.exe')) { Copy-Item -LiteralPath $env:CONTINUUM_FIXTURE_CHILD -Destination (Join-Path $release $name) }
}
function Mod-LlamaServer {
    param($RepoRoot,$InstallDirectory)
    New-Item -ItemType Directory -Path $InstallDirectory -Force | Out-Null
    Copy-Item -LiteralPath $env:CONTINUUM_FIXTURE_CHILD -Destination (Join-Path $InstallDirectory 'llama-server.exe')
}
'@
        $modules.Replace('__MODULES__', (Join-Path $repo 'tools\scripts\lib\win-modules.ps1').Replace("'", "''")) |
            Set-Content -LiteralPath (Join-Path $prepareLib 'win-modules.ps1')
        $missingFiles = @{cmake=(Join-Path $cmakeBin 'cmake.exe'); llvm=(Join-Path $llvmBin 'libclang.dll'); cuda=(Join-Path $cudaBin 'nvcc.exe')}
        foreach ($extra in @('', ' -Update', ' -Grid', ' -ResumePrepared', 'cmake', 'llvm', 'cuda')) {
            $missing = $missingFiles[$extra]
            if ($missing) { Remove-Item -LiteralPath $missing }
            $info = [Diagnostics.ProcessStartInfo]::new((Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'))
            $info.Arguments = '-NoProfile -ExecutionPolicy RemoteSigned -File "' + (Join-Path $prepareRepo 'install.ps1') + '" -PrepareOnly'
            if (-not $missing) { $info.Arguments += $extra }
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
                if (-not $extra) {
                    if ($process.ExitCode -ne 0 -or $output -notmatch 'fixture prebuilt validated') { throw "Public preparation failed: $output" }
                } elseif ($missing) {
                    if ($process.ExitCode -eq 0 -or $output -notmatch 'Preparation requires' -or $output -match 'Unexpected download') { throw "Missing cached toolchain did not fail before provisioning: $output" }
                } elseif ($process.ExitCode -eq 0 -or $output -notmatch 'cannot be combined') { throw 'Preparation accepted incompatible flags' }
            } finally {
                $process.Dispose()
                if ($missing) { Copy-Item -LiteralPath $child -Destination $missing }
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
    $runner = Join-Path $repo 'tools\scripts\run-service-hidden.ps1'
    $core = Join-Path $scratch 'core with spaces.exe'
    $socket = Join-Path $scratch 'socket with spaces.sock'
    & $shell -NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File $runner -ExecutablePath $child -CorePath $core -SocketPath $socket -EnginePath $child -LogDirectory $logs
    if ($LASTEXITCODE -ne 7) { throw "Hidden host lost child exit code: $LASTEXITCODE" }
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
        $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum-core-server.exe'; ExecutablePath = ('\\?\' + $output) })
        Protect-CoreBuildOutput -TargetDirectory $target
        if ($held.HasExited -or (Test-Path $output)) { throw 'Busy output was not preserved live under its previous name' }
        Copy-Item -LiteralPath $child -Destination $output
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
