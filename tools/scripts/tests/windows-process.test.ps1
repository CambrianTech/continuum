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
} finally {
    # Only this test's newly-created, resolved scratch directory is removed.
    $resolved = [IO.Path]::GetFullPath($scratch)
    $parent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $resolved.StartsWith($parent, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe scratch path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
exit 0
