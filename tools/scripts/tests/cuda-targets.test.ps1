$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\..\lib\cuda-targets.ps1"
$saved = $env:CUDA_PATH; $savedCap = $env:CUDA_COMPUTE_CAP
try {
    $env:CUDA_PATH = ''; $env:CUDA_COMPUTE_CAP = ''
    function nvidia-smi { $global:LASTEXITCODE = 0; $script:caps }
    function nvcc { $global:LASTEXITCODE = 0; 'compute_61','compute_75','compute_86','compute_89','compute_120' }
    function Invoke-InstallerProcess {
        param($FilePath, $ArgumentList, [switch]$OwnProcessTree)
        if (-not $OwnProcessTree) { throw 'CUDA setup probes must use owned hidden launch.' }
        if ($FilePath -eq 'nvidia-smi') { nvidia-smi }
        elseif ($FilePath -eq 'nvcc') { nvcc }
        else { throw "Unexpected CUDA probe: $FilePath" }
    }
    # Regression: GTX1080Ti/1070, RTX20/3090/40/5090 and mixed fleets must not
    # inherit one developer's GPU. Minimum PTX, all distinct CMake architectures.
    foreach ($case in @(
        @{ Caps=@('6.1','6.1'); Expected='61'; Ptx='61' },
        @{ Caps=@('7.5'); Expected='75'; Ptx='75' },
        @{ Caps=@('8.6'); Expected='86'; Ptx='86' },
        @{ Caps=@('8.9'); Expected='89'; Ptx='89' },
        @{ Caps=@('12.0'); Expected='120'; Ptx='120' },
        @{ Caps=@('12.0','8.6','6.1'); Expected='61;86;120'; Ptx='61' }
    )) {
        $script:caps=$case.Caps
        $actual=Get-CudaTargets
        if ($actual.CMake -ne $case.Expected -or $actual.Candle -ne $case.Ptx) { throw 'Incorrect detected target set' }
    }
    foreach($bad in @('N/A','6','6.10','99.0','')) {
        $script:caps=@($bad); $refused=$false
        try { Get-CudaTargets | Out-Null } catch { $refused=$true }
        if(-not $refused){throw "Accepted invalid/unsupported target $bad"}
    }
    $script:caps=@('6.1'); $env:CUDA_COMPUTE_CAP='120'
    $refused=$false; try { Get-CudaTargets | Out-Null } catch {$refused=$true}
    if(-not $refused){throw 'Accepted incompatible override'}
    Write-Output 'PASS CUDA device families, mixed devices, unsupported/malformed targets and incompatible override'
} finally { $env:CUDA_PATH=$saved; $env:CUDA_COMPUTE_CAP=$savedCap }
exit 0
