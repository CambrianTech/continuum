# Regression: installer native commands must retain arguments, drain both pipes,
# and preserve failures without allocating a console in desktop harnesses.
param([switch]$CheckDescendants, [string]$PrebuiltCli)
$ErrorActionPreference = 'Stop'
# A long-lived host may have loaded the previous helper ABI already. The new
# launcher must not bind its completed-exit call to that cached zero-arg type.
if (-not ('Continuum.Setup.OwnedProcess' -as [type])) {
    Add-Type -TypeDefinition 'namespace Continuum.Setup { public sealed class OwnedProcess { public void CompleteHandoff() {} } }'
}
. "$PSScriptRoot/../lib/windows-elevation.ps1"
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('continuum-process-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
    # Card2041ea54: legacy bridge migration may close ONLY its own unused
    # single-client stream. A core/Scheduler winning accept must remain alive.
    & {
        . "$PSScriptRoot/../lib/windows-media-reconciliation.ps1"
        $media = Join-Path $scratch 'legacy-media'
        New-Item -ItemType Directory -Path $media | Out-Null
        $bridge = Join-Path $media 'livekit-bridge.exe'
        Add-Type -OutputAssembly $bridge -OutputType ConsoleApplication -TypeDefinition @'
using System;
using System.IO;
using System.Net;
using System.Net.Sockets;
public class SingleClientBridgeFixture {
    public static void Main(string[] args) {
        var listener=new TcpListener(IPAddress.Loopback,0);
        listener.Start(); Console.WriteLine(((IPEndPoint)listener.LocalEndpoint).Port);
        using(var client=listener.AcceptTcpClient()) {
            File.WriteAllText(args[0],"accepted");
            while(client.GetStream().ReadByte()!=-1) {}
        }
        listener.Stop();
    }
}
'@
        Invoke-InstallerProcess -OwnProcessTree "$env:SystemRoot\System32\cmd.exe" @('/d','/c','exit','0')
        Initialize-CoreMediaHandle
        $nativeConnections = ${function:Get-CoreMediaConnections}
        foreach ($scenario in @('idle','active','racing-core')) {
            $accepted = Join-Path $media "$scenario.accepted"
            $start = [Diagnostics.ProcessStartInfo]::new()
            $start.FileName=$bridge; $start.Arguments='"' + $accepted + '"'; $start.WorkingDirectory=$media
            $start.UseShellExecute=$false; $start.CreateNoWindow=$true
            $start.RedirectStandardOutput=$true; $start.RedirectStandardError=$true
            $owned = [Continuum.Setup.OwnedProcessV2]::Start($start)
            $primary = $null; $fixtureProcess = $null
            try {
                $line=$owned.StandardOutput.ReadLineAsync()
                if (-not $line.Wait(5000)) { throw 'Media fixture did not listen.' }
                $port=[int]$line.Result
                $listener=@(Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction Stop)
                $fixtureProcess=[Diagnostics.Process]::GetProcessById($listener[0].OwningProcess)
                $identity=[Continuum.Setup.HeldMediaProcess]::new([uint32]$fixtureProcess.Id)
                try {
                    $plan=[pscustomobject]@{schema=1;installRoot=$media;image=$bridge;imageSha256=(Get-FileHash $bridge).Hash;
                        processId=$fixtureProcess.Id;createdUtcMicroseconds=$identity.CreatedUtcMicroseconds;port=$port}
                } finally { $identity.Dispose() }
                if ($scenario -eq 'idle') {
                    foreach ($field in @('image','imageSha256','createdUtcMicroseconds','port')) {
                        $saved=$plan.$field
                        $plan.$field=switch($field) { 'image' { Join-Path $media 'wrong.exe' } 'imageSha256' { '0'*64 } 'createdUtcMicroseconds' { [long]1 } 'port' { 1 } }
                        $refused=$false
                        try { Close-CoreIdleLegacyMedia -Plan $plan -ExitTimeoutMs 250 }
                        catch { if ($_.Exception.Message -notmatch 'identity changed|image changed|endpoint changed') { throw }; $refused=$true }
                        finally { $plan.$field=$saved }
                        if (-not $refused -or $owned.WaitForExit(0) -or (Test-Path $accepted)) { throw "Unsafe media $field acceptance." }
                    }
                    $bound=Join-Path $media 'plan.json'
                    [IO.File]::WriteAllText($bound,($plan | ConvertTo-Json -Compress))
                    $refused=$false
                    try { Invoke-CoreLegacyMediaPlan -Path $bound -Sha ('0'*64) }
                    catch { if ($_.Exception.Message -notmatch 'consented digest') { throw }; $refused=$true }
                    if (-not $refused) { throw 'Swapped elevation plan was accepted.' }
                    $refused=$false
                    try { Invoke-CoreLegacyMediaReconciliation -Image $bridge -InstallRoot $media }
                    catch { if ($_.Exception.Message -notmatch 'PrepareOnly preserves') { throw }; $refused=$true }
                    if (-not $refused -or (Test-Path $accepted)) { throw 'PrepareOnly changed media lifecycle.' }
                    & {
                        # Only elevation is replaced here. The real caller builds
                        # its plan from CIM and the image hash, and must dispatch
                        # the owning module rather than its importing script.
                        $expectedCreated=$plan.createdUtcMicroseconds
                        $expectedModule=[IO.Path]::GetFullPath("$PSScriptRoot/../lib/windows-media-reconciliation.ps1")
                        function Get-NetTCPConnection { [pscustomobject]@{State='Listen';LocalAddress='127.0.0.1';LocalPort=9101;OwningProcess=$fixtureProcess.Id} }
                        function Get-CoreMediaConnections { @{tcp=@(Get-NetTCPConnection);udp=@()} }
                        function Invoke-Elevated {
                            param($CommandLine,$Reason)
                            if ($CommandLine[6] -ne $expectedModule -or $CommandLine[7] -ne '-MediaPlanPath' -or $CommandLine[9] -ne '-MediaPlanSha') { throw 'Wrong elevated media owner/arguments.' }
                            $boundPlan=[IO.File]::ReadAllText($CommandLine[8]) | ConvertFrom-Json
                            if ($boundPlan.createdUtcMicroseconds -ne $expectedCreated -or
                                $boundPlan.image -ne $bridge -or $boundPlan.processId -ne $fixtureProcess.Id -or
                                (Get-FileHash $CommandLine[8]).Hash -ne $CommandLine[10]) { throw 'Elevation lost process identity or plan binding.' }
                            Write-Output 'fixture reconciliation receipt'
                            $global:LASTEXITCODE=0
                        }
                        $data=@(Invoke-CoreLegacyMediaReconciliation -Image $bridge -InstallRoot $media -AllowElevation)
                        if ($data.Count -or (Test-Path $accepted)) { throw 'Reconciliation diagnostic contaminated release data or caller opened a socket.' }
                    }
                } else {
                    $primary=[Net.Sockets.TcpClient]::new()
                    if ($scenario -eq 'active') { $primary.Connect('127.0.0.1',$port) }
                    else {
                        # Deterministic Scheduler/start race AFTER the real idle
                        # observation, BEFORE reconciliation's own connection.
                        function Get-CoreMediaConnections {
                            param([int]$ProcessId)
                            $snapshot=& $nativeConnections -ProcessId $ProcessId
                            $primary.Connect('127.0.0.1',$port)
                            return $snapshot
                        }
                    }
                }
                $refused=$false
                try { Close-CoreIdleLegacyMedia -Plan $plan -ExitTimeoutMs 250 | Out-Null }
                catch {
                    if ($scenario -eq 'idle' -or $_.Exception.Message -notmatch 'is active|retained a client') { throw }
                    $refused=$true
                }
                if ($scenario -eq 'idle') {
                    if ($refused -or -not $owned.WaitForExit(5000) -or $owned.ExitCode -ne 0) { throw 'Idle bridge did not exit cleanly.' }
                } else {
                    if (-not $refused -or $owned.WaitForExit(0) -or -not $primary.Connected) { throw 'Reconciliation interrupted the first client.' }
                    # Actual first stream remains usable; only its eventual EOF
                    # ends this single-client server, not the queued second EOF.
                    $primary.GetStream().WriteByte(42)
                    $primary.Dispose(); $primary=$null
                    if (-not $owned.WaitForExit(5000) -or $owned.ExitCode -ne 0) { throw 'Preserved client lost normal disconnect behavior.' }
                }
            } finally {
                ${function:Get-CoreMediaConnections}=$nativeConnections
                if ($primary) { $primary.Dispose() }
                $owned.Dispose()
                if ($fixtureProcess) { $fixtureProcess.Dispose() }
            }
        }
        Write-Host 'PASS: pinned legacy media EOF retires idle bridge, refuses mismatches, and preserves active/racing first client.'
    }
    if ($PrebuiltCli) {
        # Real published CLI regression: verified-archive progress must not
        # contaminate the JSON consumed by Get-CorePrebuiltRelease. This uses
        # its actual downloaded-archive path, not a canned child response.
        $checkout = Join-Path $scratch 'checkout'
        $download = Join-Path $scratch 'download'
        $fixtureHome = Join-Path $scratch 'home'
        $payload = Join-Path $download 'continuum-core-windows-x86_64'
        New-Item -ItemType Directory -Force -Path "$checkout/tools/scripts/lib",$payload,$fixtureHome | Out-Null
        [IO.File]::WriteAllText("$checkout/tools/scripts/lib/core-features.sh", 'select_core_features() { CONTINUUM_HARDWARE_FEATURES=""; }')
        & git -C $checkout init --quiet
        & git -C $checkout add .
        & git -C $checkout -c user.name=Fixture -c user.email=fixture@example.invalid commit --quiet -m fixture
        if ($LASTEXITCODE -ne 0) { throw 'Cannot create prebuilt fixture checkout.' }
        $fork = (& git -C $checkout rev-parse HEAD).Trim()
        & git -C $checkout update-index --add --cacheinfo "160000,$fork,core/vendor/llama.cpp"
        & git -C $checkout -c user.name=Fixture -c user.email=fixture@example.invalid commit --quiet -m fork
        if ($LASTEXITCODE -ne 0) { throw 'Cannot record fixture engine identity.' }
        $tip = (& git -C $checkout rev-parse HEAD).Trim()
        $bins = @('continuum-core-server','continuum','continuum-mcp','forge-custodian')
        foreach ($bin in $bins) { [IO.File]::WriteAllText((Join-Path $payload "$bin.exe"), 'archive fixture; never executed') }
        $archive = Join-Path $download 'continuum-core-windows-x86_64.tar.gz'
        # GNU tar interprets C: as a remote host. Match the production tar_on
        # boundary: archive basename and members are relative to its directory.
        Push-Location $download
        try { & tar.exe -czf (Split-Path $archive -Leaf) 'continuum-core-windows-x86_64' }
        finally { Pop-Location }
        if ($LASTEXITCODE -ne 0) { throw 'Cannot create prebuilt fixture archive.' }
        $manifest = @{platform='windows-x86_64'; git_sha=$tip; features=''; archive=(Split-Path $archive -Leaf); sha256=(Get-FileHash $archive).Hash.ToLowerInvariant(); bins=$bins; runtime_libs=@(); engine=@{source_revision=$fork;backend='cuda';relative_path='engine'}}
        $manifestPath = Join-Path $download 'continuum-core-windows-x86_64.json'
        $child = Join-Path $scratch 'prepare-child.ps1'
        @'
param($Cli, $Checkout, $Download, $FixtureHome)
# Only this child changes its cache/log root, never the test host or user state.
$env:HOME = $FixtureHome
$env:USERPROFILE = $FixtureHome
& $Cli prepare-prebuilt $Checkout $Download
exit $LASTEXITCODE
'@ | Set-Content -LiteralPath $child -Encoding UTF8
        # Initialize and reuse the installer's job-owned adapter: timeout disposal
        # kills the CLI and tar/git descendants, then reaps before scratch cleanup.
        Invoke-InstallerProcess -OwnProcessTree "$env:SystemRoot\System32\cmd.exe" @('/d','/c','exit','0')
        foreach ($corrupt in @($false,$true)) {
            if ($corrupt) { $manifest.sha256 = '0' * 64 }
            [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 4), [Text.UTF8Encoding]::new($false))
            $start = New-Object Diagnostics.ProcessStartInfo
            $start.FileName = "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"
            $start.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "' + $child + '" "' + [IO.Path]::GetFullPath($PrebuiltCli) + '" "' + $checkout + '" "' + $download + '" "' + $fixtureHome + '"'
            $start.WorkingDirectory = $scratch
            $start.UseShellExecute = $false; $start.CreateNoWindow = $true
            $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
            $process = [Continuum.Setup.OwnedProcessV2]::Start($start)
            try {
                $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
                if (-not $process.WaitForExit(30000)) { throw 'Real prebuilt preparation timed out.' }
                if ($corrupt) {
                    if ($process.ExitCode -eq 0 -or $stdout.Result.Trim() -or $stderr.Result -notmatch 'sha256') { throw 'Invalid archive emitted success data or lost its checksum refusal.' }
                } else {
                    if ($process.ExitCode -ne 0) { throw "Real preparation refused: $($stderr.Result)" }
                    $result = $stdout.Result | ConvertFrom-Json
                    if ($result.git_sha -cne $tip -or -not (Test-Path -LiteralPath $result.core) -or $stderr.Result -notmatch 'deploy-consume: CI build') { throw 'Preparation lost its JSON identity, extracted core or separate diagnostic.' }
                }
            } finally { $process.Dispose() }
        }
        Write-Host 'PASS: actual prepare-prebuilt keeps verified-archive diagnostics separate from JSON and refuses corruption.'
        # The published supervisor must initialize real Windows PowerShell. Its
        # canonical extended executable spelling previously failed before -File.
        # Keep this fixture entirely outside the user's install and task state;
        # inert core/engine files make accidental service launch fail closed.
        . "$PSScriptRoot/../lib/windows-prepared.ps1"
        $serviceHome = Join-Path $scratch 'supervisor-home'
        $installRoot = Join-Path $serviceHome '.continuum'
        $serviceSlot = Join-Path $installRoot 'bin/service-a'
        $engineSlot = Join-Path $installRoot 'bin/engine-a'
        $programFiles = Join-Path $scratch 'program-files'
        $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $cliHash = (Get-FileHash -LiteralPath $PrebuiltCli).Hash.ToLowerInvariant()
        $bootstrap = Join-Path $programFiles "Continuum/$sid/supervisor-$cliHash"
        New-Item -ItemType Directory -Force -Path $serviceSlot,$engineSlot,$bootstrap | Out-Null
        Copy-Item -LiteralPath $PrebuiltCli -Destination (Join-Path $serviceSlot 'continuum.exe')
        Copy-Item -LiteralPath $PrebuiltCli -Destination (Join-Path $bootstrap 'continuum.exe')
        $publishedDirectory = Split-Path ([IO.Path]::GetFullPath($PrebuiltCli)) -Parent
        foreach ($name in @(Get-Content -LiteralPath (Join-Path $publishedDirectory 'runtime-libs.txt'))) {
            if ($name -notmatch '^[A-Za-z0-9_.-]+\.dll$') { throw 'Invalid published runtime fixture input.' }
            Copy-Item -LiteralPath (Join-Path $publishedDirectory $name) -Destination (Join-Path $bootstrap $name)
        }
        [IO.File]::WriteAllText((Join-Path $serviceSlot 'continuum-core-server.exe'), 'inert fixture: never execute')
        [IO.File]::WriteAllText((Join-Path $engineSlot 'llama-server.exe'), 'inert fixture: never execute')
        @'
param($ExecutablePath, $CorePath, $SocketPath, $EnginePath, $LogDirectory, $EyeRoot)
[System.Net.ServicePointManager]::SecurityProtocol | Write-Output
Write-Output 'DIAGNOSTIC_ONLY_NO_CORE'
exit 0
'@ | Set-Content -LiteralPath (Join-Path $serviceSlot 'run-service-hidden.ps1') -Encoding UTF8
        $release = [pscustomobject]@{
            artifact=(Join-Path $serviceSlot 'continuum-core-server.exe'); cli=(Join-Path $serviceSlot 'continuum.exe')
            launcher=(Join-Path $serviceSlot 'run-service-hidden.ps1'); engine=(Join-Path $engineSlot 'llama-server.exe')
            socket=(Join-Path $installRoot 'unused.sock'); logDirectory=(Join-Path $installRoot 'logs')
        }
        Save-CorePreparedRelease -Release $release -InstallRoot $installRoot -Selection Active
        $supervisorChild = Join-Path $scratch 'supervisor-child.ps1'
        @'
param($Cli, $FixtureHome, $FixtureProgramFiles, $Receipt)
$env:HOME = $FixtureHome
$env:USERPROFILE = $FixtureHome
$env:ProgramFiles = $FixtureProgramFiles
$env:ProgramW6432 = $FixtureProgramFiles
# Windows derives the native ProgramFiles value from ProgramW6432 at launch.
& $Cli installed-service core $Receipt
exit $LASTEXITCODE
'@ | Set-Content -LiteralPath $supervisorChild -Encoding UTF8
        $start = New-Object Diagnostics.ProcessStartInfo
        $start.FileName = "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"
        $start.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy RemoteSigned -File "' + $supervisorChild + '" "' + (Join-Path $bootstrap 'continuum.exe') + '" "' + $serviceHome + '" "' + $programFiles + '" "' + (Join-Path $installRoot 'install-active.json') + '"'
        $start.WorkingDirectory = $scratch
        $start.UseShellExecute = $false; $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
        $process = [Continuum.Setup.OwnedProcessV2]::Start($start)
        try {
            $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(30000)) { throw 'Published supervisor diagnostic timed out.' }
            if ($process.ExitCode -ne 0) { throw "Published supervisor refused: $($stderr.Result)" }
            $launchLog = Get-Content -LiteralPath (Join-Path $release.logDirectory 'service-bootstrap.log') -Raw
            if ($launchLog -notmatch 'DIAGNOSTIC_ONLY_NO_CORE' -or $launchLog -match 'ServicePointManager.*exception') { throw "Published PowerShell boundary failed: $launchLog" }
            if (Test-Path -LiteralPath $release.socket) { throw 'Diagnostic launcher unexpectedly created a core socket.' }
        } finally { $process.Dispose() }
        Write-Host 'PASS: actual published supervisor initializes PowerShell and exits without starting a core.'
        return
    }
    # Regression: hidden PS5 -File lost Cargo stderr when its host rendered
    # unmerged ErrorRecords. Test OS pipes across two real PS5 boundaries.
    $boundaryProbe = Join-Path $scratch 'boundary.ps1'
    @'
param($Helper, $Depth, $Mode)
. $Helper
Invoke-InstallerEntryPoint {
    if ([int]$Depth -gt 0) {
        Invoke-InstallerProcess -OwnProcessTree -PreserveChildrenOnSuccess "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" @('-NoProfile', '-ExecutionPolicy', 'RemoteSigned', '-File', $PSCommandPath, $Helper, ([string]([int]$Depth - 1)), $Mode)
        exit $global:LASTEXITCODE
    }
    $data = @(Invoke-InstallerProcess -OwnProcessTree "$env:SystemRoot\System32\cmd.exe" -RawArguments '/d /c "echo DATA & echo DIAGNOSTIC 1>&2 & exit /b 23"')
    if ($data.Count -ne 1 -or $data[0].Trim() -ne 'DATA') { throw 'Native stderr contaminated success data.' }
    Write-Output $data
    if ($Mode -eq 'throw') { throw 'TERMINATING DIAGNOSTIC' }
    exit $global:LASTEXITCODE
}
'@ | Set-Content -LiteralPath $boundaryProbe -Encoding UTF8
    foreach ($mode in @('exit', 'throw')) {
        $start = New-Object Diagnostics.ProcessStartInfo
        $start.FileName = "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"
        $start.Arguments = '-NoProfile -ExecutionPolicy RemoteSigned -File "' + $boundaryProbe + '" "' + [IO.Path]::GetFullPath("$PSScriptRoot/../lib/windows-elevation.ps1") + '" 1 ' + $mode
        $start.UseShellExecute = $false; $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
        $process = [Diagnostics.Process]::Start($start)
        try {
            $out = $process.StandardOutput.ReadToEndAsync(); $err = $process.StandardError.ReadToEndAsync()
            if (-not $process.WaitForExit(30000)) { $process.Kill(); throw 'Diagnostic boundary fixture timed out.' }
            $wanted = if ($mode -eq 'throw') { 1 } else { 23 }
            if ($process.ExitCode -ne $wanted) { throw "Nested exit changed: $($process.ExitCode), wanted $wanted; $($err.Result)" }
            if ($out.Result.Trim() -ne 'DATA') { throw "Success stream changed: $($out.Result)" }
            if ([regex]::Matches($err.Result, '(?m)^DIAGNOSTIC\s*$').Count -ne 1) { throw "Missing/duplicate native diagnostic: $($err.Result)" }
            if ([regex]::Matches($err.Result, 'TERMINATING DIAGNOSTIC').Count -ne [int]($mode -eq 'throw')) { throw "Missing/duplicate terminating diagnostic: $($err.Result)" }
        } finally { $process.Dispose() }
    }
    # Public argument validation runs before any provisioning or lease writes.
    $publicErrors = @(Invoke-InstallerProcess -OwnProcessTree "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" @('-NoProfile', '-ExecutionPolicy', 'RemoteSigned', '-File', "$PSScriptRoot/../../../install.ps1", '-PrepareOnly', '-Grid') 2>&1)
    if ($global:LASTEXITCODE -ne 1 -or @($publicErrors | Where-Object { $_.ToString() -match 'PrepareOnly cannot be combined' }).Count -ne 1) { throw "Public entry lost its terminating error: $publicErrors" }
    Write-Host 'PASS: hidden nested PS5 entries preserve separate diagnostics, data, and exits.'

    # Regression: an inherited module search path must not choose another
    # PowerShell engine's built-ins (the public PS5 Get-Acl failure on BIGGIEDESK).
    $foreignModules = Join-Path $scratch 'foreign-modules'
    $foreignSecurity = Join-Path $foreignModules 'Microsoft.PowerShell.Security'
    New-Item -ItemType Directory -Path $foreignSecurity -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $foreignSecurity 'Microsoft.PowerShell.Security.psm1'), 'function Get-Acl { throw "FOREIGN SECURITY MODULE" }; Export-ModuleMember -Function Get-Acl')
    [IO.File]::WriteAllText((Join-Path $foreignSecurity 'Microsoft.PowerShell.Security.psd1'), "@{ RootModule='Microsoft.PowerShell.Security.psm1'; ModuleVersion='99.0'; FunctionsToExport=@('Get-Acl') }")
    $moduleProbe = Join-Path $scratch 'module-probe.ps1'
    @'
param($Helper, $Foreign, $Target, $Mode)
$ErrorActionPreference = 'Stop'
$env:PSModulePath = $Foreign + ';' + $env:PSModulePath
$inherited = $env:PSModulePath
try {
    if ($Mode -eq 'fixed') { . $Helper }
    $null = Get-Acl -LiteralPath $Target
    if ($Mode -ne 'fixed') { throw 'Negative control did not select the foreign module.' }
    if ($env:PSModulePath -cne $inherited) { throw 'Installer discarded user module paths.' }
    [Console]::WriteLine('NATIVE MODULE PASS')
} catch {
    if ($Mode -eq 'control' -and $_.Exception.Message -match 'FOREIGN SECURITY MODULE') { [Console]::WriteLine('FOREIGN CONTROL PASS'); exit 0 }
    [Console]::Error.WriteLine($_.Exception.ToString()); exit 1
}
'@ | Set-Content -LiteralPath $moduleProbe -Encoding UTF8
    foreach ($mode in @('control', 'fixed')) {
        $result = @(Invoke-InstallerProcess -OwnProcessTree "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" @('-NoProfile', '-File', $moduleProbe, "$PSScriptRoot/../lib/windows-elevation.ps1", $foreignModules, $scratch, $mode))
        $expected = if ($mode -eq 'fixed') { 'NATIVE MODULE PASS' } else { 'FOREIGN CONTROL PASS' }
        if ($LASTEXITCODE -ne 0 -or $result -notcontains $expected) { throw "Inherited module regression failed ($mode): $result" }
    }
    Write-Host 'PASS: installer selects runtime built-ins without rewriting inherited module paths.'
    # Run this portion on the CI desktop only: it intentionally probes a tool
    # which launches its own child without supplying any window-hiding flags.
    # It must not be used as an experiment on an operator's active desktop.
    if ($CheckDescendants) {
        if ($env:GITHUB_ACTIONS -ne 'true') { throw 'Descendant visibility probe requires the CI desktop.' }
        $grandchild = Join-Path $scratch 'grandchild.ps1'
        @'
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class WindowProbe { [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow(); [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr handle); }'
if ([WindowProbe]::IsWindowVisible([WindowProbe]::GetConsoleWindow())) { [Console]::Out.WriteLine('VISIBLE DESCENDANT'); exit 91 }
[Console]::Out.WriteLine('HIDDEN DESCENDANT')
'@ | Set-Content -LiteralPath $grandchild -Encoding UTF8
        $parent = Join-Path $scratch 'parent.ps1'
        @'
param($Grandchild)
$start = New-Object Diagnostics.ProcessStartInfo
$start.FileName = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$start.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $Grandchild + '"'
$start.UseShellExecute = $false
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
# Deliberately no CreateNoWindow or WindowStyle. This models an unadapted tool.
$child = [Diagnostics.Process]::Start($start)
try {
    $stdout = $child.StandardOutput.ReadToEndAsync()
    $stderr = $child.StandardError.ReadToEndAsync()
    if (-not $child.WaitForExit(30000)) { $child.Kill(); throw 'Descendant probe timed out.' }
    [Console]::Out.Write($stdout.Result)
    [Console]::Error.Write($stderr.Result)
    exit $child.ExitCode
} finally { $child.Dispose() }
'@ | Set-Content -LiteralPath $parent -Encoding UTF8
        $visibility = @(Invoke-InstallerProcess "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $parent, $grandchild))
        if ($LASTEXITCODE -ne 0 -or $visibility -notcontains 'HIDDEN DESCENDANT') { throw "Unadapted descendant was visible or failed: $visibility" }
        Write-Host 'PASS: ordinary descendant remains hidden without its own creation flags.'
    }
    $child = Join-Path $scratch 'native child.ps1'
    @'
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class ConsoleProbe { [DllImport("kernel32.dll")] public static extern IntPtr GetConsoleWindow(); }'
if ([ConsoleProbe]::GetConsoleWindow() -ne [IntPtr]::Zero) { exit 91 }
if ([Environment]::CommandLine.Contains('"--probe"')) { exit 92 }
foreach ($value in $args) { [Console]::Out.WriteLine('ARG:' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($value))) }
$line = 'x' * 4096
for ($i = 0; $i -lt 256; $i++) {
    [Console]::Out.WriteLine('OUT:' + $line)
    [Console]::Error.WriteLine('ERR:' + $line)
}
exit 23
'@ | Set-Content -LiteralPath $child -Encoding UTF8
    $expected = @('--probe', '', 'with spaces', 'embedded"quote', 'C:\path with spaces\', 'backslash\"quote', '$literal; & |')
    $actual = New-Object 'System.Collections.Generic.List[string]'
    $counts = @{ stdout = 0; stderr = 0 }
    Invoke-InstallerProcess -OwnProcessTree -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
        -ArgumentList (@('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $child) + $expected) 2>&1 |
        ForEach-Object {
            $line = $_.ToString()
            if ($line.StartsWith('ARG:')) { $actual.Add([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($line.Substring(4)))) }
            elseif ($line.StartsWith('OUT:')) { $counts.stdout++ }
            elseif ($line.StartsWith('ERR:')) { $counts.stderr++ }
            else { throw "Unexpected child output: $line" }
        }
    if ($LASTEXITCODE -ne 23) { throw "Native failure lost or child had a console: $LASTEXITCODE" }
    if ($counts.stdout -ne 256 -or $counts.stderr -ne 256) { throw 'A redirected pipe lost output.' }
    if ($actual.Count -ne $expected.Count) { throw 'Argument count changed.' }
    for ($i = 0; $i -lt $expected.Count; $i++) {
        if ($actual[$i] -cne $expected[$i]) { throw "Argument $i changed: '$($actual[$i])'." }
    }
    Write-Host 'PASS: hidden native launch, exact argv, dual-pipe draining, native failure.'
    # What this catches: Get-Command -CommandType Application returns multiple
    # matches when desktop harnesses and the user both supply the same tool.
    $savedPath = $env:PATH
    try {
        $firstTool = Join-Path $scratch 'first'
        $secondTool = Join-Path $scratch 'second'
        New-Item -ItemType Directory -Path $firstTool,$secondTool | Out-Null
        foreach ($directory in @($firstTool,$secondTool)) {
            Copy-Item -LiteralPath $env:ComSpec -Destination (Join-Path $directory 'duplicate-tool.exe')
        }
        $env:PATH = "$firstTool;$secondTool;$savedPath"
        Invoke-InstallerProcess -OwnProcessTree 'duplicate-tool.exe' -RawArguments '/d /c exit 19'
        if ($LASTEXITCODE -ne 19) { throw 'Duplicate PATH matches broke native command resolution.' }
    } finally { $env:PATH = $savedPath }
    Write-Host 'PASS: duplicate native PATH candidates use the first command.'
    # A cancelled build owns its descendants, unlike intentional service launch.
    # Both fixture generations explicitly hide their windows on operator desktops.
    $ownedChild = Join-Path $scratch 'owned tree.ps1'
    @'
param($Receipt)
$start = New-Object Diagnostics.ProcessStartInfo
$start.FileName = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
$start.Arguments = '-NoProfile -NonInteractive -Command "Start-Sleep -Seconds 120"'
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$descendant = [Diagnostics.Process]::Start($start)
@($PID, $descendant.Id) | Set-Content -LiteralPath $Receipt
[Console]::Out.WriteLine('READY')
Start-Sleep -Seconds 120
'@ | Set-Content -LiteralPath $ownedChild -Encoding UTF8
    $receipt = Join-Path $scratch 'owned-pids.txt'
    $caught = $false
    try {
        Invoke-InstallerProcess -OwnProcessTree -PreserveChildrenOnExitCode @(0,200) "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" @('-NoProfile', '-NonInteractive', '-File', $ownedChild, $receipt) |
            ForEach-Object { if ($_ -eq 'READY') { throw 'cancel fixture' } }
    } catch {
        if ($_.Exception.Message -notmatch 'cancel fixture') { throw }
        $caught = $true
    }
    if (-not $caught -or -not (Test-Path -LiteralPath $receipt)) { throw 'Cancellation fixture did not start.' }
    foreach ($ownedId in (Get-Content -LiteralPath $receipt)) {
        $remaining = Get-Process -Id ([int]$ownedId) -ErrorAction SilentlyContinue
        if ($remaining -and -not $remaining.WaitForExit(5000)) { throw "Cancelled installer left owned process $ownedId running." }
    }
    Write-Host 'PASS: downstream cancellation terminates owned child and grandchild.'
    # Stopping a silent pipeline must also reach the launcher's finally block.
    # Close the actual pipe handles (including those inherited by descendants)
    # before waiting, so this also exercises the post-EOF process-wait path.
    $closedPipesChild = Join-Path $scratch 'closed pipes.ps1'
    @'
param($Receipt)
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class PipeCloser { [DllImport("kernel32.dll")] public static extern IntPtr GetStdHandle(int which); [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle); }'
[void][PipeCloser]::CloseHandle([PipeCloser]::GetStdHandle(-11))
[void][PipeCloser]::CloseHandle([PipeCloser]::GetStdHandle(-12))
$PID | Set-Content -LiteralPath $Receipt
Start-Sleep -Seconds 120
'@ | Set-Content -LiteralPath $closedPipesChild -Encoding UTF8
    $silentReceipt = Join-Path $scratch 'silent-pids.txt'
    $pipeline = [PowerShell]::Create()
    try {
        [void]$pipeline.AddScript({
            param($Helper, $Fixture, $Receipt)
            . $Helper
            Invoke-InstallerProcess -OwnProcessTree "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" @('-NoProfile', '-NonInteractive', '-File', $Fixture, $Receipt)
        }).AddArgument((Join-Path $PSScriptRoot '../lib/windows-elevation.ps1')).AddArgument($closedPipesChild).AddArgument($silentReceipt)
        $pending = $pipeline.BeginInvoke()
        $deadline = [DateTime]::UtcNow.AddSeconds(15)
        while (-not (Test-Path -LiteralPath $silentReceipt) -and [DateTime]::UtcNow -lt $deadline -and -not $pending.IsCompleted) { Start-Sleep -Milliseconds 50 }
        if (-not (Test-Path -LiteralPath $silentReceipt)) { throw "Silent fixture did not start: $($pipeline.Streams.Error)" }
        $stop = $pipeline.BeginStop($null, $null)
        if (-not $stop.AsyncWaitHandle.WaitOne(10000)) { throw 'Silent pipeline cancellation did not finish within ten seconds.' }
        $pipeline.EndStop($stop)
        foreach ($ownedId in (Get-Content -LiteralPath $silentReceipt)) {
            $remaining = Get-Process -Id ([int]$ownedId) -ErrorAction SilentlyContinue
            if ($remaining -and -not $remaining.WaitForExit(5000)) { throw "Silent cancellation left owned process $ownedId running." }
        }
    } finally { $pipeline.Dispose() }
    Write-Host 'PASS: silent pipeline cancellation after pipe EOF terminates owned tree within bounded time.'
    $rejected = $false
    try { Invoke-InstallerProcess -PreserveChildrenOnExitCode @(200) 'must-not-launch.exe' } catch {
        if ($_.Exception.Message -notmatch 'requires an owned process tree') { throw }
        $rejected = $true
    }
    if (-not $rejected) { throw 'Completed outcome accepted without process ownership.' }
    # Coordinators may hand off only explicitly accepted completed outcomes. Keep a
    # process handle to this fixture child so cleanup cannot target a reused PID.
    $handoff = Join-Path $scratch 'handoff.ps1'
    @'
param($Receipt, [int]$Code)
Add-Type -TypeDefinition @"
using System; using System.Text; using System.ComponentModel; using System.Runtime.InteropServices;
public static class HandoffChild {
 [StructLayout(LayoutKind.Sequential)] struct SI { public uint cb; public IntPtr a,b,c; public uint d,e,f,g,h,i,j,k; public ushort l,m; public IntPtr n,o,p,q; }
 [StructLayout(LayoutKind.Sequential)] struct PI { public IntPtr process,thread; public uint id,tid; }
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcessW(string exe,StringBuilder command,IntPtr pa,IntPtr ta,bool inherit,uint flags,IntPtr env,string cwd,ref SI si,out PI pi);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 public static uint Start(string exe) { var si=new SI(); si.cb=(uint)Marshal.SizeOf(typeof(SI)); PI pi;
 if(!CreateProcessW(exe,new StringBuilder("\""+exe+"\" -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 30\""),IntPtr.Zero,IntPtr.Zero,false,0x08000000,IntPtr.Zero,null,ref si,out pi)) throw new Win32Exception();
 CloseHandle(pi.thread); CloseHandle(pi.process); return pi.id; }
}
"@
# Like AIRC's daemon, this child has no inherited installer output handles.
$childId=[HandoffChild]::Start((Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'))
$childId | Set-Content -LiteralPath $Receipt
[Console]::Out.WriteLine('HANDOFF:' + $childId)
exit $Code
'@ | Set-Content -LiteralPath $handoff -Encoding UTF8
    foreach ($case in @(@{ Code=0; Allow=@(); Survives=$true }, @{ Code=23; Allow=@(); Survives=$false }, @{ Code=200; Allow=@(); Survives=$false }, @{ Code=200; Allow=@(200); Survives=$true }, @{ Code=23; Allow=@(200); Survives=$false })) {
        $code = $case.Code
        $adopted = $null
        try {
            Invoke-InstallerProcess -OwnProcessTree -PreserveChildrenOnSuccess -PreserveChildrenOnExitCode $case.Allow "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" @('-NoProfile','-NonInteractive','-File',$handoff,$receipt,"$code") |
                ForEach-Object { if ($_ -match '^HANDOFF:(\d+)$') { $adopted = Get-Process -Id ([int]$Matches[1]); $null = $adopted.Handle } }
            if ($LASTEXITCODE -ne $code -or -not $adopted) { throw 'Handoff fixture did not report its child and exit.' }
            if ($case.Survives -and $adopted.HasExited) { throw 'Successful coordinator killed its adopted daemon.' }
            if (-not $case.Survives -and -not $adopted.WaitForExit(5000)) { throw 'Failed coordinator preserved its child.' }
        } finally {
            if ($adopted) { if (-not $adopted.HasExited) { $adopted.Kill(); $adopted.WaitForExit() }; $adopted.Dispose() }
        }
    }
    Write-Host 'PASS: explicit completed outcomes preserve children and native failures; unlisted failures retain kill ownership.'
    # vcvars is a batch file. CRT argument escaping must not corrupt cmd's
    # quoted executable path or its redirection/conditional command syntax.
    $batch = Join-Path $scratch 'environment fixture.cmd'
    '@set CONTINUUM_HIDDEN_PROCESS_FIXTURE=imported' | Set-Content -LiteralPath $batch -Encoding ASCII
    $environment = @(Invoke-InstallerProcess $env:ComSpec -RawArguments "/d /s /c `"`"$batch`" >nul 2>&1 && set CONTINUUM_HIDDEN_PROCESS_FIXTURE`"")
    if ($LASTEXITCODE -ne 0 -or $environment -notcontains 'CONTINUUM_HIDDEN_PROCESS_FIXTURE=imported') { throw 'Hidden batch environment import failed.' }
    if ($env:CONTINUUM_HIDDEN_PROCESS_FIXTURE) { throw 'Fixture changed the caller environment.' }
    Write-Host 'PASS: hidden batch shell grammar and environment output.'
    # An unsuccessful device login must stop grid setup, not warn and install a
    # node that cannot join its owner's account. This fixture cannot provision.
    & {
        . "$PSScriptRoot/../lib/install-common.ps1"
        . "$PSScriptRoot/../lib/win-modules.ps1"
        function Install-IfMissing { }
        function Get-Command { [pscustomobject]@{ Source = 'fixture-gh' } }
        function Module-Start { }
        function Module-Done { throw 'Failed login was marked complete.' }
        $script:authCommands = @()
        function Invoke-InstallerProcess {
            param($FilePath, $ArgumentList)
            if ($FilePath -ne 'gh') { throw 'Unexpected acquisition.' }
            $script:authCommands += ($ArgumentList -join ' ')
            $global:LASTEXITCODE = if ($ArgumentList[1] -eq 'status') { 1 } else { 23 }
        }
        $failure = ''
        try { Mod-GhAuth -WantsGrid } catch { $failure = $_.Exception.Message }
        if ($failure -notmatch 'exit 23' -or $script:authCommands.Count -ne 2 -or
            $script:authCommands[1] -cne 'auth login --hostname github.com --git-protocol https --web') {
            throw "Grid authentication failure was bypassed: $failure"
        }
    }
    Write-Host 'PASS: device authentication uses hidden launch and failed login stops grid setup.'
    # The remote entry must carry exactly the same helper before it can clone.
    # Prove the CI check accepts the real projection and refuses a changed copy.
    $fixtureRepo = Join-Path $scratch 'bootstrap repository'
    $fixtureScripts = Join-Path $fixtureRepo 'tools\scripts'
    New-Item -ItemType Directory -Path (Join-Path $fixtureScripts 'lib') -Force | Out-Null
    Copy-Item -LiteralPath "$PSScriptRoot/../lib/windows-elevation.ps1" -Destination (Join-Path $fixtureScripts 'lib')
    Copy-Item -LiteralPath "$PSScriptRoot/../lib/windows-prebuilt.ps1" -Destination (Join-Path $fixtureScripts 'lib')
    $fixtureArtifactSource = Join-Path $fixtureRepo 'core/continuum-cli-lifecycle/src/prebuilt_artifact.rs'
    New-Item -ItemType Directory -Path (Split-Path $fixtureArtifactSource) -Force | Out-Null
    Copy-Item -LiteralPath "$PSScriptRoot/../../../core/continuum-cli-lifecycle/src/prebuilt_artifact.rs" -Destination $fixtureArtifactSource
    Copy-Item -LiteralPath "$PSScriptRoot/../sync-windows-bootstrap.ps1" -Destination $fixtureScripts
    $fixtureEntry = Join-Path $fixtureRepo 'install.ps1'
    Copy-Item -LiteralPath "$PSScriptRoot/../../../install.ps1" -Destination $fixtureEntry
    $checkArgs = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $fixtureScripts 'sync-windows-bootstrap.ps1'), '-Check')
    Invoke-InstallerProcess "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" $checkArgs | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Canonical bootstrap projection was refused.' }
    $changed = [IO.File]::ReadAllText($fixtureEntry).Replace('$start.CreateNoWindow = $true', '$start.CreateNoWindow = $false')
    [IO.File]::WriteAllText($fixtureEntry, $changed)
    Invoke-InstallerProcess "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" $checkArgs 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) { throw 'Divergent bootstrap launcher passed drift check.' }
    Copy-Item -LiteralPath "$PSScriptRoot/../../../install.ps1" -Destination $fixtureEntry
    $fixturePrebuilt = Join-Path $fixtureScripts 'lib/windows-prebuilt.ps1'
    $changed = [IO.File]::ReadAllText($fixturePrebuilt).Replace("'Cargo.lock'", "'wrong-build-input'")
    [IO.File]::WriteAllText($fixturePrebuilt, $changed)
    Invoke-InstallerProcess "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" $checkArgs 2>&1 | Out-Null
    if ($LASTEXITCODE -eq 0) { throw 'Divergent prebuilt input projection passed drift check.' }
    Write-Host 'PASS: bootstrap drift check accepts canonical source and rejects changed launcher or prebuilt inputs.'
} finally {
    # Only this test's newly-created, resolved scratch directory is removed.
    $resolved = [IO.Path]::GetFullPath($scratch)
    $parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolved.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe scratch path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
exit 0
