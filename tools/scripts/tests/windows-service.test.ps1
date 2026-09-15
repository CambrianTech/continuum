# Regression for #4056: exercise installed-slot safety and the real hidden
# supervisor without registering tasks or changing the operator's environment.
# Run with Windows PowerShell 5.1 (also the scheduler's production runtime).
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $repo 'tools\scripts\lib\windows-service.ps1')
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('continuum-service-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
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
    $registeredRelease.engine = Join-Path $installed 'bin\engine-b\llama-server.exe'
    $script:registeredTask = [pscustomobject]@{ Description = ($registeredRelease | ConvertTo-Json -Compress) }
    $script:liveProcesses = @([pscustomobject]@{ Name = 'llama-server.exe'; ExecutablePath = (Join-Path $installed 'bin\engine-a\llama-server.exe') })
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
        $script:liveProcesses = @([pscustomobject]@{ Name = 'continuum-core-server.exe'; ExecutablePath = $output })
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
