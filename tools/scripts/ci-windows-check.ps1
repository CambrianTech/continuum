# Run the native MSVC type-check without linking resident binaries or running custody tests.
# The workflow owns cache restoration; this command is identical on a non-serving dev machine.
$ErrorActionPreference = 'Stop'
if (-not $env:CARGO_TARGET_DIR) { throw 'Set CARGO_TARGET_DIR to the configured shared build cache before checking.' }
$checkTimer = [Diagnostics.Stopwatch]::StartNew()
& cargo check --locked --target x86_64-pc-windows-msvc --profile dev-fast -p continuum-core -p continuum-cli-lifecycle --lib --tests --features continuum-core/test-fixtures
$checkExit = $LASTEXITCODE
$checkTimer.Stop()
Write-Output "Windows type-check elapsed seconds: $($checkTimer.Elapsed.TotalSeconds)"
if ($checkExit -ne 0) { exit $checkExit }
