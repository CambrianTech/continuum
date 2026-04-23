# enable-tailscale-ssh.ps1 — one-time-setup, idempotent. Windows/PowerShell.
#
# Run this on a host (BigMama, Windows dev box, anything you want others
# to reach) and from then on, any device on your Tailnet can SSH in
# WITHOUT a per-device key. Tailscale handles auth via your Tailnet
# identity + ACLs instead of OpenSSH's per-device authorized_keys.
#
# Usage (Windows PowerShell):
#   pwsh scripts\enable-tailscale-ssh.ps1
#
# No admin required.

$ErrorActionPreference = 'Stop'

# Locate tailscale.exe. On Windows it's usually installed here; fall back
# to PATH if someone has a non-standard install.
$candidates = @(
  "$Env:ProgramFiles\Tailscale\tailscale.exe",
  "$Env:ProgramFiles(x86)\Tailscale\tailscale.exe"
)
$tsExe = $null
foreach ($c in $candidates) {
  if (Test-Path $c) { $tsExe = $c; break }
}
if (-not $tsExe) {
  $onPath = Get-Command tailscale -ErrorAction SilentlyContinue
  if ($onPath) { $tsExe = $onPath.Source }
}
if (-not $tsExe) {
  Write-Error "tailscale CLI not found. Install from https://tailscale.com/download and re-run."
  exit 1
}

Write-Host "-> tailscale CLI: $tsExe"

# Confirm the daemon is reachable.
& $tsExe status | Out-Null
if ($LASTEXITCODE -ne 0) {
  Write-Warning "tailscale daemon not responding. Running 'tailscale status' for diagnosis:"
  & $tsExe status
  Write-Host ""
  Write-Host "Most likely fix: open the Tailscale tray app to authenticate this machine."
  Write-Host "Then re-run this script."
  exit 1
}

# The actual fix. `tailscale up --ssh` preserves previously-set flags
# (advertise-routes, accept-routes, etc.) and is idempotent.
Write-Host "-> Enabling Tailscale SSH (idempotent, preserves other flags)..."
& $tsExe up --ssh
if ($LASTEXITCODE -ne 0) {
  Write-Error "tailscale up --ssh failed. See output above."
  exit $LASTEXITCODE
}

$hostName = $Env:COMPUTERNAME
$tsIp = (& $tsExe ip -4 | Select-Object -First 1)

Write-Host ""
Write-Host "✓ Tailscale SSH enabled on this host."
Write-Host "  hostname:     $hostName"
Write-Host "  tailscale ip: $tsIp"
Write-Host ""
Write-Host "Teammates on your Tailnet can now reach this host with:"
Write-Host ""
Write-Host "  tailscale ssh <user>@$hostName"
Write-Host "  # or by IP:"
Write-Host "  tailscale ssh <user>@$tsIp"
Write-Host ""
Write-Host "No per-device SSH keys needed — Tailnet identity + ACL is the auth."
