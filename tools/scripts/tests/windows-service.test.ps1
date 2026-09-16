# Regression for #4056: exercise installed-slot safety and the real hidden
# supervisor without registering tasks or changing the operator's environment.
# Run with Windows PowerShell 5.1 (also the scheduler's production runtime).
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $repo 'tools\scripts\lib\windows-service.ps1')
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('continuum-service-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
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
        @{ userSid = $callerSid; shell = 'fixture'; arguments = 'fixture'; description = 'fixture' } |
            ConvertTo-Json | Set-Content -LiteralPath $planPath -Encoding UTF8
        . (Join-Path $repo 'tools\scripts\register-core-service.ps1') -PlanPath $planPath
        if ($script:aclRegistrations -ne 1 -or -not (Test-CoreServiceCallerAccess -Sddl $script:aclTask.Sddl -UserSid $callerSid)) {
            throw 'Registrar did not persist and verify the caller grant'
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
