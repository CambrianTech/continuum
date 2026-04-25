# bootstrap.ps1 -- back-compat redirect to install.ps1.
# Continuum's canonical Windows installer is now install.ps1.
# See docs/INSTALL-ARCHITECTURE.md for the design.

Write-Host ''
Write-Host '  bootstrap.ps1 is now a redirect to install.ps1 (the canonical'
Write-Host '  Windows installer). Forwarding ...'
Write-Host ''

& "$PSScriptRoot\install.ps1" @args
exit $LASTEXITCODE
