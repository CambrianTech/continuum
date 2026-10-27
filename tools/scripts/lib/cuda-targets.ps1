# Native adapter for the CUDA target contract: detect every local GPU, validate
# against the selected compiler, build CMake fat binaries and minimum-capability
# Candle PTX (forward-compatible with newer devices). Never assume a GPU model.
function Get-CudaTargets {
    $caps = @(Invoke-InstallerProcess -OwnProcessTree 'nvidia-smi' @('--query-gpu=compute_cap', '--format=csv,noheader') 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "CUDA device capability query failed: $caps" }
    $architectures = @($caps | ForEach-Object {
        $value = "$_".Trim()
        if ($value -notmatch '^([1-9][0-9]*)\.([0-9])$') { throw "Invalid CUDA device capability: $value" }
        [int]($Matches[1] + $Matches[2])
    } | Sort-Object -Unique)
    if (-not $architectures.Count) { throw 'No CUDA device capabilities detected.' }
    # Both toolkit layouts: NVIDIA's (bin\) and conda's (Library\bin\), as cuda-targets.sh.
    $compiler = if ($env:CUDA_PATH) {
        $found = @('bin\nvcc.exe', 'Library\bin\nvcc.exe') | ForEach-Object { Join-Path $env:CUDA_PATH $_ } |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
        if (-not $found) { throw "No nvcc under CUDA_PATH=$($env:CUDA_PATH) (bin\ or Library\bin\)" }
        $found
    } else { 'nvcc' }
    $supported = @(Invoke-InstallerProcess -OwnProcessTree $compiler @('--list-gpu-arch') 2>&1)
    if ($LASTEXITCODE -ne 0) { throw "CUDA compiler architecture query failed: $supported" }
    foreach ($arch in $architectures) {
        if ($supported.Trim() -notcontains "compute_$arch") {
            throw "Selected CUDA toolkit cannot compile for detected compute_$arch. Install a toolkit supporting this device; CPU fallback is not permitted."
        }
    }
    $ptx = $architectures[0]
    if ($env:CUDA_COMPUTE_CAP) {
        if ($env:CUDA_COMPUTE_CAP -notmatch '^[1-9][0-9]+$' -or
            [int]$env:CUDA_COMPUTE_CAP -gt $ptx -or
            $supported.Trim() -notcontains "compute_$($env:CUDA_COMPUTE_CAP)") {
            throw 'CUDA_COMPUTE_CAP override cannot run on all detected devices or is unsupported by the selected toolkit.'
        }
        $ptx = [int]$env:CUDA_COMPUTE_CAP
    }
    [pscustomobject]@{ CMake = ($architectures -join ';'); Candle = "$ptx" }
}

function Set-CudaTargets {
    $targets = Get-CudaTargets
    $env:CMAKE_CUDA_ARCHITECTURES = $targets.CMake
    $env:CUDA_COMPUTE_CAP = $targets.Candle
    Write-Host "  + CUDA targets: CMake=$($targets.CMake), Candle PTX=$($targets.Candle)"
    return $targets
}
