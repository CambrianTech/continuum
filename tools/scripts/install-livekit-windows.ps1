param([string]$Version = '1.13.7')
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Expected a numeric LiveKit version' }
$arch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    'X64' { 'amd64' }
    'Arm64' { 'arm64' }
    default { throw 'Unsupported Windows architecture' }
}
$destination = Join-Path $env:USERPROFILE '.continuum/bin/livekit-server.exe'
if (Test-Path -LiteralPath $destination) {
    & $destination --version
    if ($LASTEXITCODE -ne 0) { throw 'Installed LiveKit binary failed its version check' }
    exit 0
}
$cache = Join-Path $env:USERPROFILE ".continuum/cache/livekit/$Version"
New-Item -ItemType Directory -Force -Path $cache | Out-Null
$name = "livekit_${Version}_windows_${arch}.zip"
$archive = Join-Path $cache $name
$base = "https://github.com/livekit/livekit/releases/download/v$Version"
Invoke-WebRequest -UseBasicParsing -Uri "$base/$name" -OutFile "$archive.partial"
$checksums = (Invoke-WebRequest -UseBasicParsing -Uri "$base/checksums.txt").Content
if ($checksums -is [byte[]]) { $checksums = [Text.Encoding]::UTF8.GetString($checksums) }
$entry = @($checksums -split "`n" | Where-Object { $_.Trim() -match ('\s+\*?' + [regex]::Escape($name) + '$') })
if ($entry.Count -ne 1) { throw 'Release checksum entry missing or ambiguous' }
$expected = ($entry[0].Trim() -split '\s+')[0]
if ((Get-FileHash -LiteralPath "$archive.partial" -Algorithm SHA256).Hash -ne $expected) {
    throw 'LiveKit archive checksum mismatch'
}
Move-Item -LiteralPath "$archive.partial" -Destination $archive -Force
Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $cache 'expanded') -Force
$binary = Join-Path $cache 'expanded/livekit-server.exe'
if (!(Test-Path -LiteralPath $binary)) { throw 'Release archive lacks livekit-server.exe' }
& $binary --version
if ($LASTEXITCODE -ne 0) { throw 'Downloaded LiveKit binary failed its version check' }
New-Item -ItemType Directory -Force -Path (Split-Path $destination) | Out-Null
Copy-Item -LiteralPath $binary -Destination $destination
Write-Output "Installed verified LiveKit $Version at $destination"
