# Regression: installer native commands must retain arguments, drain both pipes,
# and preserve failures without allocating a console in desktop harnesses.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/../lib/windows-elevation.ps1"
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('continuum-process-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $scratch | Out-Null
try {
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
    Invoke-InstallerProcess -FilePath "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
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
    Write-Host 'PASS: bootstrap drift check accepts canonical source and rejects changed launcher.'
} finally {
    # Only this test's newly-created, resolved scratch directory is removed.
    $resolved = [IO.Path]::GetFullPath($scratch)
    $parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolved.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe scratch path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
exit 0
