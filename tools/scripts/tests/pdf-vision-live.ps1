# Opt-in acceptance against the already-running core and its bound model.
# No engine startup, model selection fallback, installation, or private documents.
param(
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [string]$Continuum = 'continuum',
    [string]$ExpectedBuildSha = ''
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force $root | Out-Null
# Invalidate a previous pass before any fallible runtime call. Reusing an output
# directory for a failed deployment check must never leave a current-looking pass.
@{passed=$false; status='incomplete'; startedAt=[DateTime]::UtcNow.ToString('o')} |
    ConvertTo-Json | Set-Content (Join-Path $root 'receipt.json')
function Invoke-Core([string]$Command, [object]$Params) {
    $json = ConvertTo-Json -InputObject $Params -Depth 16 -Compress
    $raw = & $Continuum $Command $json
    if ($LASTEXITCODE -ne 0) { throw "$Command failed with exit $LASTEXITCODE" }
    return ($raw | Out-String | ConvertFrom-Json)
}
$coreBefore = Invoke-Core 'ping' @{}
if (-not $coreBefore.ok -or -not $coreBefore.buildSha) { throw 'Running core did not provide build provenance' }
if ($ExpectedBuildSha -and $coreBefore.buildSha -ne $ExpectedBuildSha) {
    throw "Expected deployed build $ExpectedBuildSha, running $($coreBefore.buildSha); refusing acceptance against a stale core"
}
$status = Invoke-Core 'ai/inference/status' @{}
if (-not $status.ready -or -not $status.activeModel) { throw 'Existing bound model is not ready' }
$binding = Invoke-Core 'ai/model-info' @{model=$status.activeModel}
if ($binding.modelInfo.id -ne $status.activeModel -or -not $binding.provider -or
    $binding.modelInfo.capabilities -notcontains 'vision') {
    throw 'Exact active model metadata must declare native vision before PDF inference'
}
$binding | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $root 'binding.json')
# Vector-only PDF: the text extractor cannot disclose the expected answer.
$stream = '1 1 1 rg 0 0 400 300 re f 0 0 1 rg 55 80 90 90 re f 1 0 0 rg 315 125 m 315 150 295 170 270 170 c 245 170 225 150 225 125 c 225 100 245 80 270 80 c 295 80 315 100 315 125 c f'
$objects = @(
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 300] /Resources << >> /Contents 4 0 R >>',
    ("<< /Length " + $stream.Length + " >>`nstream`n" + $stream + "`nendstream")
)
$pdf = "%PDF-1.4`n"
$offsets = @(0)
for ($i=0; $i -lt $objects.Count; $i++) {
    $offsets += $pdf.Length
    $pdf += "$($i+1) 0 obj`n$($objects[$i])`nendobj`n"
}
$xref = $pdf.Length
$pdf += "xref`n0 5`n0000000000 65535 f `n"
for ($i=1; $i -le 4; $i++) { $pdf += ('{0:0000000000} 00000 n ' -f $offsets[$i]) + "`n" }
$pdf += "trailer`n<< /Size 5 /Root 1 0 R >>`nstartxref`n$xref`n%%EOF`n"
$source = Join-Path $root 'visual.pdf'
[IO.File]::WriteAllText($source, $pdf, [Text.Encoding]::ASCII)
$target = ([Uri]$source).AbsoluteUri + '#page=1'
$observe = Invoke-Core 'perception/observe' @{target=$target;viewport=@{width=720;height=720}}
$observe | ConvertTo-Json -Depth 16 | Set-Content (Join-Path $root 'observe.json')
if (-not $observe.success -or -not $observe.image.dataUrl) { throw 'PDF observation did not return pixels' }
if ($observe.structure.children[0].text) { throw 'Visual-only fixture unexpectedly has a text layer' }
$imageBytes = [Convert]::FromBase64String(($observe.image.dataUrl -split ',',2)[1])
$imagePath = Join-Path $root 'page.png'
[IO.File]::WriteAllBytes($imagePath,$imageBytes)
$request = @{
    model=$binding.modelInfo.id; provider=$binding.provider; temperature=0; maxTokens=384
    messages=@(@{role='user';content=@(
        @{type='text';text='Inspect this PDF page image. State the color and shape on the left, then the color and shape on the right. Answer only those visual facts in one sentence.'},
        @{type='image';image=@{url=$observe.image.dataUrl}}
    )})
}
$request | ConvertTo-Json -Depth 16 | Set-Content (Join-Path $root 'request.json')
$clock = [Diagnostics.Stopwatch]::StartNew()
$result = Invoke-Core 'ai/generate' $request
$clock.Stop()
$result | ConvertTo-Json -Depth 16 | Set-Content (Join-Path $root 'result.json')
# Check both facts and ordering; a plausible generic image description cannot pass.
if (-not $result.success -or $result.text -notmatch '(?is)blue\s+square.*red\s+circle') {
    throw 'Model did not identify the left blue square and right red circle; inspect result.json'
}
if ($result.model -ne $binding.modelInfo.id -or $result.provider -ne $binding.provider) {
    throw 'Visual answer came from a different model/provider than the selected binding'
}
$coreAfter = Invoke-Core 'ping' @{}
if (-not $coreAfter.ok -or $coreAfter.buildSha -ne $coreBefore.buildSha -or
    $coreAfter.buildNumber -ne $coreBefore.buildNumber) {
    throw 'Core revision changed during visual acceptance; result cannot attest one deployed build'
}
@{
    passed=$true; model=$result.model; provider=$result.provider
    boundModel=$binding.modelInfo.id; boundCapabilities=$binding.modelInfo.capabilities
    coreBuildSha=$coreAfter.buildSha; coreBuildNumber=$coreAfter.buildNumber
    requestId=$result.requestId; elapsedMs=$clock.ElapsedMilliseconds
    sourceSha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
    imageSha256=(Get-FileHash -LiteralPath $imagePath -Algorithm SHA256).Hash
    textLayerEmpty=$true; answer=$result.text
} | ConvertTo-Json | Set-Content (Join-Path $root 'receipt.json')
Write-Output "PASS: PDF pixels reached the bound model and its answer matched the visual fixture. Receipt: $root"
