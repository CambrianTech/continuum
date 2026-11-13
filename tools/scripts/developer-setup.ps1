# Developer-only preflight and optional GitHub workflow authorization.
# Windows PowerShell 5.1 or PowerShell 7 (Windows/macOS/Linux).
[CmdletBinding()]
param([switch]$AuthorizeWorkflows)
$ErrorActionPreference = 'Stop'

if ($AuthorizeWorkflows) {
    foreach ($tool in @('airc', 'gh')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            throw "$tool is required; see CONTRIBUTING.md developer setup."
        }
    }
    # Environment tokens override gh's keyring. Refresh the interactive developer
    # credential without changing saved settings or any running service environment.
    $saved = @{}
    foreach ($name in @('GH_TOKEN', 'GITHUB_TOKEN')) {
        $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        [Environment]::SetEnvironmentVariable($name, $null, 'Process')
    }
    try {
        & airc gh run -- auth refresh --hostname github.com --scopes workflow
        if ($LASTEXITCODE -ne 0) { throw 'GitHub workflow authorization did not complete; follow the reported error before retrying.' }
        & airc gh run -- auth status --hostname github.com
        if ($LASTEXITCODE -ne 0) { throw 'GitHub credential verification failed.' }
        Write-Host 'Authorization refreshed. Check that the reported scopes include workflow.'
    } finally {
        foreach ($name in $saved.Keys) {
            [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process')
        }
    }
    Write-Host 'Existing GH_TOKEN/GITHUB_TOKEN overrides were restored. A push using an older override still uses that older credential; see CONTRIBUTING.md.'
    return
}

$missing = @()
foreach ($tool in @('git', 'rustup', 'cargo', 'cmake', 'node', 'npm', 'airc', 'gh')) {
    $command = Get-Command $tool -ErrorAction SilentlyContinue
    if ($command) { Write-Host "FOUND $tool : $($command.Source)" }
    else { $missing += $tool; Write-Host "NOT ON PATH $tool" }
}
Write-Host 'Native compiler/SDK and optional GPU requirements: CONTRIBUTING.md developer setup.'
if ($env:CARGO_TARGET_DIR) { Write-Host "Shared Cargo target: $env:CARGO_TARGET_DIR" }
else { Write-Host 'Set CARGO_TARGET_DIR to the existing machine shared cache before building (see AGENTS.md).' }
Write-Host 'This is an inventory, not a build or readiness certification. No services, repositories, caches, or credentials were changed.'
if ($missing.Count) { throw "Developer tools not found on this shell's PATH: $($missing -join ', '). Follow CONTRIBUTING.md; installed SDK tools may need their developer shell." }
