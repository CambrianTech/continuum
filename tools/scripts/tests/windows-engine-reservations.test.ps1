# Receipt reservations must reach the native lane-aware selector unchanged.
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $repo 'tools\scripts\lib\windows-service.ps1')
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) 'continuum-reservation-fixture'
function Get-ManagedPayloadRoot { $fixtureRoot }
function Invoke-InstallerProcess {
    param($FilePath, $ArgumentList)
    $global:LASTEXITCODE = 0
    if ($ArgumentList[0] -eq '--help') { return 'continuum engine idle-slot --owner-pid <pid> [--exclude-slot slot]...' }
    # The claim is this installer's (Codex on #5535): its own pid, with the receipted exclusions.
    if (($ArgumentList -join ' ') -cne "engine idle-slot --owner-pid $PID --exclude-slot engine-a") {
        throw "Installer custody was not passed to the selector: $ArgumentList"
    }
    Join-Path $fixtureRoot 'bin\engine-b'
}
$reserved = Join-Path $fixtureRoot 'bin\engine-a'
$picked = Get-CoreEngineIdleSlot -Cli $PSCommandPath -InstallRoot $fixtureRoot -ReservedSlots @($reserved)
if ($picked -ne (Join-Path $fixtureRoot 'bin\engine-b')) { throw 'Reserved engine prevented selection of a free slot.' }
Write-Output 'PASS: installer reservations reach the native selector; its alternative is preserved'
