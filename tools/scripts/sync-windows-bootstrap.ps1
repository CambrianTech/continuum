# Keep the pre-clone entry self-contained without maintaining a second launcher.
# Author Invoke-InstallerProcess in lib/windows-elevation.ps1, then run this file.
param([switch]$Check)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$source = Join-Path $PSScriptRoot 'lib\windows-elevation.ps1'
$entry = Join-Path $root 'install.ps1'
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
$functions = @($ast.FindAll({ param($node)
    $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in @('Initialize-InstallerPowerShell', 'Invoke-InstallerEntryPoint', 'Invoke-InstallerProcess')
}, $false))
if ($functions.Count -ne 3) { throw 'Expected canonical installer environment and process functions.' }
$begin = '# BEGIN GENERATED INSTALLER PROCESS - tools/scripts/sync-windows-bootstrap.ps1'
$end = '# END GENERATED INSTALLER PROCESS'
$text = [IO.File]::ReadAllText($entry).Replace("`r`n", "`n")
$start = $text.IndexOf($begin, [StringComparison]::Ordinal)
$finish = $text.IndexOf($end, [StringComparison]::Ordinal)
if ($start -lt 0 -or $finish -le $start -or $text.LastIndexOf($begin) -ne $start -or $text.LastIndexOf($end) -ne $finish) {
    throw 'Expected one bounded generated launcher region in install.ps1.'
}
$body = (($functions | ForEach-Object { $_.Extent.Text.Replace("`r`n", "`n") }) -join "`n`n")
$rendered = $text.Substring(0, $start) + $begin + "`n" + $body + "`n" + $end + $text.Substring($finish + $end.Length)
if ($Check) {
    if ($text -cne $rendered) { throw 'Bootstrap launcher drift: run tools/scripts/sync-windows-bootstrap.ps1 and commit install.ps1.' }
    Write-Host 'PASS: bootstrap uses the canonical installer process implementation.'
} else {
    [IO.File]::WriteAllText($entry, $rendered, (New-Object Text.UTF8Encoding($false)))
}

# The first CLI is not installed yet, so project its canonical build-key inputs
# into the transport shim rather than maintaining a second policy list.
$artifactSource = [IO.File]::ReadAllText((Join-Path $root 'core/continuum-cli-lifecycle/src/prebuilt_artifact.rs'))
$inputBlock = [regex]::Match($artifactSource, '(?s)pub const BUILD_INPUTS:.*?=\s*\[(.*?)\];')
if (-not $inputBlock.Success) { throw 'Missing canonical prebuilt build inputs.' }
$inputs = @([regex]::Matches($inputBlock.Groups[1].Value, '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value })
$prebuiltPath = Join-Path $PSScriptRoot 'lib/windows-prebuilt.ps1'
$prebuiltText = [IO.File]::ReadAllText($prebuiltPath).Replace("`r`n", "`n")
$projection = '# BEGIN GENERATED PREBUILT INPUTS' + "`n" + '$script:CorePrebuiltInputs = @(' + (($inputs | ForEach-Object { "'$_'" }) -join ', ') + ')' + "`n" + '# END GENERATED PREBUILT INPUTS'
$projected = [regex]::Replace($prebuiltText, '(?s)# BEGIN GENERATED PREBUILT INPUTS.*?# END GENERATED PREBUILT INPUTS', { $projection })
if ($Check) {
    if ($prebuiltText -cne $projected) { throw 'Prebuilt input projection drift: run sync-windows-bootstrap.ps1.' }
} else { [IO.File]::WriteAllText($prebuiltPath, $projected, (New-Object Text.UTF8Encoding($false))) }
