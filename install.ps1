# install.ps1 -- Continuum native installer for Windows.
#
# Ordinary users consume verified published core and engine artifacts. Toolchain
# provisioning and source compilation require explicit -DeveloperBuild. Both feed
# the same validated staging, provision-once supervisor, and guarded handoff owner.
#
# Usage:
#   # Remote one-liner (obtains the installer, then the published native release):
#   irm https://raw.githubusercontent.com/CambrianTech/continuum/main/install.ps1 | iex
#
#   # From a checkout:
#   powershell -ExecutionPolicy Bypass -File .\install.ps1          # local-only
#   powershell -ExecutionPolicy Bypass -File .\install.ps1 -Grid    # + GitHub login for grid
#   powershell -ExecutionPolicy RemoteSigned -File .\install.ps1 -Update # update the selected tracking branch
#   powershell -ExecutionPolicy RemoteSigned -File .\install.ps1 -DeveloperBuild # contributor source build
#
# Docker remains available as a RUNTIME for grid nodes (docker compose up); it is
# NOT a second install path. COUNTERPART: tools/scripts/install.sh (Unix). A
# change to the install CONTRACT belongs in both.

[CmdletBinding()]
param(
    [switch]$Grid,
    [switch]$Update,
    [switch]$ResumePrepared,
    [switch]$PrepareOnly,
    [switch]$DeveloperBuild
)

# BEGIN GENERATED INSTALLER PROCESS - tools/scripts/sync-windows-bootstrap.ps1
function Initialize-InstallerPowerShell {
    # A PS7 desktop host may pass its PSModulePath to Windows PowerShell 5.
    # Load the running engine's built-ins explicitly before autoload can select
    # another engine's Security/Utility type data. Keep user module paths intact.
    foreach ($name in @('Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility', 'Microsoft.PowerShell.Security')) {
        $manifest = [IO.Path]::Combine($PSHOME, 'Modules', $name, ($name + '.psd1'))
        Import-Module $manifest -Global -ErrorAction Stop
    }
}

function Invoke-InstallerEntryPoint {
    param([Parameter(Mandatory = $true)][scriptblock]$Action)
    try {
        & $Action 2>&1 | ForEach-Object {
            if ($_ -is [Management.Automation.ErrorRecord]) {
                [Console]::Error.WriteLine($_.ToString())
            } else { Write-Output $_ }
        }
    } catch {
        [Console]::Error.WriteLine($_.ToString())
        exit 1
    }
}

function Invoke-InstallerProcess {
    [CmdletBinding(DefaultParameterSetName = 'Argv')]
    param([Parameter(Mandatory = $true, Position = 0)][string]$FilePath,
        [Parameter(ParameterSetName = 'Argv', Position = 1)][string[]]$ArgumentList = @(),
        [switch]$OwnProcessTree,
        [switch]$PreserveChildrenOnSuccess,
        # Explicit coordinator outcomes may verify restored runtime yet report failure.
        # This never changes the returned exit code or applies to cancellation.
        [int[]]$PreserveChildrenOnExitCode = @(),
        # cmd.exe /c uses shell grammar rather than CommandLineToArgvW. Only
        # fixed installer shell expressions should use this explicit boundary.
        [Parameter(Mandatory = $true, ParameterSetName = 'Raw')][string]$RawArguments)
    if (($PreserveChildrenOnSuccess -or $PreserveChildrenOnExitCode.Count -gt 0) -and -not $OwnProcessTree) { throw 'Completed daemon handoff requires an owned process tree.' }
    $command = Get-Command $FilePath -CommandType Application -ErrorAction Stop | Select-Object -First 1
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $command.Source
    $quoted = foreach ($arg in $ArgumentList) {
        # Windows CommandLineToArgvW quoting, including empty arguments and
        # backslashes before quotes or a closing quote.
        # Some tools (including gsudo) inspect the raw command line themselves.
        # Leave simple flags bare, as native PowerShell invocation does.
        if ($arg.Length -gt 0 -and $arg -notmatch '[\s"]') { $arg }
        else { '"' + ([regex]::Replace([regex]::Replace($arg, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1')) + '"' }
    }
    $start.Arguments = if ($PSCmdlet.ParameterSetName -eq 'Raw') { $RawArguments } else { $quoted -join ' ' }
    $start.WorkingDirectory = (Get-Location).Path
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    if ($OwnProcessTree -and -not ('Continuum.Setup.OwnedProcessV2' -as [type])) {
        # Bootstrap adapter for the same Windows job/explicit-handle-list contract
        # used by continuum-cli-lifecycle/windows_launch.rs. The kernel assigns
        # ownership before the child's first instruction, not after Process.Start.
        Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Text;
namespace Continuum.Setup {
public sealed class OwnedProcessV2 : IDisposable {
    [StructLayout(LayoutKind.Sequential)] struct Basic { public long User, Job; public uint Flags; public UIntPtr Min, Max; public uint Count; public UIntPtr Affinity; public uint Priority, Scheduling; }
    [StructLayout(LayoutKind.Sequential)] struct IO { public ulong A,B,C,D,E,F; }
    [StructLayout(LayoutKind.Sequential)] struct Limits { public Basic Basic; public IO IO; public UIntPtr ProcessMemory, JobMemory, PeakProcess, PeakJob; }
    [StructLayout(LayoutKind.Sequential)] struct Startup { public uint Size; public IntPtr Reserved, Desktop, Title; public uint X,Y,Width,Height,CharsX,CharsY,Fill,Flags; public ushort Show, ReservedSize; public IntPtr ReservedBytes, Input, Output, Error; }
    [StructLayout(LayoutKind.Sequential)] struct StartupEx { public Startup Info; public IntPtr Attributes; }
    [StructLayout(LayoutKind.Sequential)] struct ProcessInfo { public IntPtr Process, Thread; public uint Id, ThreadId; }
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr CreateJobObjectW(IntPtr attributes, IntPtr name);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job, int kind, ref Limits limits, uint size);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool InitializeProcThreadAttributeList(IntPtr list, int count, int flags, ref IntPtr size);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool UpdateProcThreadAttribute(IntPtr list, uint flags, IntPtr attribute, IntPtr value, IntPtr size, IntPtr previous, IntPtr returned);
    [DllImport("kernel32.dll")] static extern void DeleteProcThreadAttributeList(IntPtr list);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] static extern bool CreateProcessW(string application, StringBuilder command, IntPtr processAttributes, IntPtr threadAttributes, bool inherit, uint flags, IntPtr environment, string directory, ref StartupEx startup, out ProcessInfo process);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", SetLastError=true)] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool GetExitCodeProcess(IntPtr process, out uint code);
    IntPtr job, process;
    AnonymousPipeServerStream output, error, input;
    public StreamReader StandardOutput { get; private set; }
    public StreamReader StandardError { get; private set; }
    public bool HasExited { get { return WaitForSingleObject(process, 0) == 0; } }
    public int ExitCode { get { uint code; if (!GetExitCodeProcess(process, out code)) throw new Win32Exception(); return unchecked((int)code); } }
    public bool WaitForExit(int milliseconds) { uint result=WaitForSingleObject(process, (uint)milliseconds); if (result==0xFFFFFFFF) throw new Win32Exception(); return result==0; }
    public void WaitForExit() { if (!WaitForExit(-1)) throw new InvalidOperationException("Process wait failed"); }
    public void CompleteHandoff(int completedExitCode) {
        if (!HasExited || ExitCode!=completedExitCode) throw new InvalidOperationException("Only the selected completed coordinator outcome may hand off its children");
        var limits=new Limits();
        if (!SetInformationJobObject(job,9,ref limits,(uint)Marshal.SizeOf(typeof(Limits)))) throw new Win32Exception();
    }
    public static OwnedProcessV2 Start(ProcessStartInfo start) {
        var owned = new OwnedProcessV2();
        IntPtr attributes=IntPtr.Zero, handles=IntPtr.Zero, jobs=IntPtr.Zero;
        bool initialized=false;
        try {
            owned.job=CreateJobObjectW(IntPtr.Zero, IntPtr.Zero);
            if (owned.job==IntPtr.Zero) throw new Win32Exception();
            var limits=new Limits(); limits.Basic.Flags=0x2000; // KILL_ON_JOB_CLOSE
            if (!SetInformationJobObject(owned.job,9,ref limits,(uint)Marshal.SizeOf(typeof(Limits)))) throw new Win32Exception();
            owned.output=new AnonymousPipeServerStream(PipeDirection.In, HandleInheritability.Inheritable);
            owned.error=new AnonymousPipeServerStream(PipeDirection.In, HandleInheritability.Inheritable);
            owned.input=new AnonymousPipeServerStream(PipeDirection.Out, HandleInheritability.Inheritable);
            var startup=new StartupEx(); startup.Info.Size=(uint)Marshal.SizeOf(typeof(StartupEx)); startup.Info.Flags=0x100;
            startup.Info.Input=owned.input.ClientSafePipeHandle.DangerousGetHandle();
            startup.Info.Output=owned.output.ClientSafePipeHandle.DangerousGetHandle();
            startup.Info.Error=owned.error.ClientSafePipeHandle.DangerousGetHandle();
            IntPtr bytes=IntPtr.Zero;
            InitializeProcThreadAttributeList(IntPtr.Zero,2,0,ref bytes);
            if (bytes==IntPtr.Zero) throw new Win32Exception();
            attributes=Marshal.AllocHGlobal(bytes);
            if (!InitializeProcThreadAttributeList(attributes,2,0,ref bytes)) throw new Win32Exception();
            initialized=true; startup.Attributes=attributes;
            handles=Marshal.AllocHGlobal(3*IntPtr.Size);
            Marshal.WriteIntPtr(handles,0,startup.Info.Input); Marshal.WriteIntPtr(handles,IntPtr.Size,startup.Info.Output); Marshal.WriteIntPtr(handles,2*IntPtr.Size,startup.Info.Error);
            jobs=Marshal.AllocHGlobal(IntPtr.Size); Marshal.WriteIntPtr(jobs,owned.job);
            if (!UpdateProcThreadAttribute(attributes,0,new IntPtr(0x20002),handles,new IntPtr(3*IntPtr.Size),IntPtr.Zero,IntPtr.Zero) ||
                !UpdateProcThreadAttribute(attributes,0,new IntPtr(0x2000D),jobs,new IntPtr(IntPtr.Size),IntPtr.Zero,IntPtr.Zero)) throw new Win32Exception();
            ProcessInfo info;
            if (!CreateProcessW(start.FileName,new StringBuilder("\""+start.FileName+"\" "+start.Arguments),IntPtr.Zero,IntPtr.Zero,true,0x08080400,IntPtr.Zero,start.WorkingDirectory,ref startup,out info)) throw new Win32Exception();
            owned.process=info.Process; CloseHandle(info.Thread);
            owned.output.DisposeLocalCopyOfClientHandle(); owned.error.DisposeLocalCopyOfClientHandle(); owned.input.DisposeLocalCopyOfClientHandle();
            owned.input.Dispose(); owned.input=null; // Noninteractive build/acquisition stdin EOF.
            owned.StandardOutput=new StreamReader(owned.output,Console.OutputEncoding);
            owned.StandardError=new StreamReader(owned.error,Console.OutputEncoding);
            return owned;
        } catch { owned.Dispose(); throw; }
        finally { if (initialized) DeleteProcThreadAttributeList(attributes); if (attributes!=IntPtr.Zero) Marshal.FreeHGlobal(attributes); if (handles!=IntPtr.Zero) Marshal.FreeHGlobal(handles); if (jobs!=IntPtr.Zero) Marshal.FreeHGlobal(jobs); }
    }
    public void Dispose() {
        if (job!=IntPtr.Zero) { CloseHandle(job); job=IntPtr.Zero; }
        if (process!=IntPtr.Zero) { WaitForSingleObject(process,5000); CloseHandle(process); process=IntPtr.Zero; }
        if (StandardOutput!=null) StandardOutput.Dispose(); else if (output!=null) output.Dispose();
        if (StandardError!=null) StandardError.Dispose(); else if (error!=null) error.Dispose();
        if (input!=null) input.Dispose();
    }
}}
'@
    }
    $process = $null
    try {
        if ($OwnProcessTree) { $process = [Continuum.Setup.OwnedProcessV2]::Start($start) }
        else { $process = [Diagnostics.Process]::Start($start) }
        if (-not $process) { throw "Could not start $FilePath" }
        $stdout = $process.StandardOutput.ReadLineAsync()
        $stderr = $process.StandardError.ReadLineAsync()
        while ($stdout -or $stderr) {
            $pending = @(); if ($stdout) { $pending += $stdout }; if ($stderr) { $pending += $stderr }
            $index = [Threading.Tasks.Task]::WaitAny([Threading.Tasks.Task[]]$pending, 200)
            if ($index -lt 0) { continue } # Observe PowerShell cancellation even when the child is silent.
            $finished = $pending[$index]
            $line = $finished.GetAwaiter().GetResult()
            if ([object]::ReferenceEquals($finished, $stdout)) {
                if ($null -eq $line) { $stdout = $null }
                else { Write-Output $line; $stdout = $process.StandardOutput.ReadLineAsync() }
            } else {
                if ($null -eq $line) { $stderr = $null }
                else { Write-Error -Message $line -ErrorAction Continue; $stderr = $process.StandardError.ReadLineAsync() }
            }
        }
        while (-not $process.WaitForExit(200)) { } # Pipes may close before process exit; remain cancellable.
        $global:LASTEXITCODE = $process.ExitCode
        # Until an explicitly accepted completed coordinator return, cancellation owns all
        # work. Build commands never transfer their descendants' lifetime.
        if (($PreserveChildrenOnSuccess -and $process.ExitCode -eq 0) -or $PreserveChildrenOnExitCode -contains $process.ExitCode) { $process.CompleteHandoff($process.ExitCode) }
    } finally { if ($process) { $process.Dispose() } }
}
# END GENERATED INSTALLER PROCESS

Invoke-InstallerEntryPoint {
Initialize-InstallerPowerShell

$ErrorActionPreference = 'Stop'
if ($ResumePrepared -and $Update) { throw '-ResumePrepared selects an existing release and cannot be combined with -Update.' }
if ($PrepareOnly -and ($ResumePrepared -or $Update -or $Grid)) { throw '-PrepareOnly cannot be combined with -ResumePrepared, -Update, or -Grid.' }

function Enter-ContinuumInstallLease {
    $state = Join-Path $env:USERPROFILE '.continuum'
    New-Item -ItemType Directory -Force -Path $state | Out-Null
    try {
        return [IO.File]::Open((Join-Path $state 'install.lock'),
            [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    } catch [IO.IOException] { throw 'Another installer holds the install lease. Let it finish before rerunning.' }
}

function Update-ContinuumCheckout {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $lease = Enter-ContinuumInstallLease
    try {
    # Never reset, stash, switch branches, or discard a developer's changes.
    # Pull the selected branch's configured upstream, not an invented channel.
    $dirty = @(Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $RepoRoot, 'status', '--porcelain', '--untracked-files=no'))
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect checkout before update.' }
    if ($dirty.Count -ne 0) { throw 'Update refused: tracked checkout changes must be committed or resolved first.' }
    Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $RepoRoot, 'rev-parse', '--abbrev-ref', '--symbolic-full-name', '@{upstream}')
    if ($LASTEXITCODE -ne 0) { throw 'Update refused: the selected branch has no upstream. Configure its intended tracking branch first.' }
    Invoke-InstallerProcess -OwnProcessTree 'git' @('-C', $RepoRoot, 'pull', '--ff-only')
    if ($LASTEXITCODE -ne 0) { throw 'Update did not fast-forward. Resolve the upstream/network error without discarding local work, then rerun the same installer.' }
    } finally { $lease.Dispose() }
}

#  Bootstrap: make the remote `irm | iex` one-liner work for the native build 
# When piped, $PSScriptRoot is empty and there is no repo yet. Inline the minimum
# to get one (winget + git, both per-user / no admin), clone, then re-invoke the
# cloned install.ps1 which has a real $PSScriptRoot. Mirrors the root install.sh
# bootstrapper.
if (-not $PSScriptRoot) {
    if ($ResumePrepared -or $PrepareOnly) { throw 'Prepared-release operations require a local installer checkout.' }
    Write-Host '  Continuum installer (bootstrap) -- fetching the installer checkout ...'
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Write-Host '  winget not found. Install App Installer from the Microsoft Store, then re-run.' -ForegroundColor Red
        Write-Host '    https://www.microsoft.com/store/productId/9NBLGGH4NNS1'
        exit 1
    }
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        Write-Host '  -> Installing Git (per-user) ...'
        Invoke-InstallerProcess -OwnProcessTree 'winget' @('install', '--id', 'Git.Git', '--exact', '--silent', '--accept-package-agreements', '--accept-source-agreements', '--scope', 'user')
        if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne 3010) { throw "Bootstrap Git acquisition failed (exit $LASTEXITCODE)." }
        $m = [Environment]::GetEnvironmentVariable('PATH', 'Machine'); $u = [Environment]::GetEnvironmentVariable('PATH', 'User')
        $env:PATH = "$m;$u"
    }
    $target = Join-Path $env:USERPROFILE 'continuum'
    if (-not (Test-Path (Join-Path $target '.git'))) {
        Invoke-InstallerProcess -OwnProcessTree 'git' @('clone', 'https://github.com/CambrianTech/continuum.git', $target)
        if ($LASTEXITCODE -ne 0) { throw 'Repository clone failed; the installer did not run.' }
    }
    $bootArgs = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $target 'install.ps1'))
    if ($Grid) { $bootArgs += '-Grid' }
    if ($DeveloperBuild) { $bootArgs += '-DeveloperBuild' }
    . (Join-Path $target 'tools\scripts\lib\windows-elevation.ps1')
    if ($Update) { Update-ContinuumCheckout -RepoRoot $target }
    Invoke-InstallerProcess -OwnProcessTree -PreserveChildrenOnSuccess (Get-Process -Id $PID).Path $bootArgs
    exit $LASTEXITCODE
}

#  From-checkout path 
$RepoRoot = $PSScriptRoot
$LibDir = Join-Path $RepoRoot 'tools\scripts\lib'
# Update runs before install-common loads manifest-backed elevation state.
# Import the same launch primitive without acquiring an elevation session.
. (Join-Path $LibDir 'windows-elevation.ps1')
if ($Update) {
    # Reload the installer after updating: the currently parsed script and its
    # modules still contain the previous checkout's instructions.
    Update-ContinuumCheckout -RepoRoot $RepoRoot
    $updatedArgs = @('-NoProfile', '-ExecutionPolicy', 'RemoteSigned', '-File', (Join-Path $RepoRoot 'install.ps1'))
    if ($Grid) { $updatedArgs += '-Grid' }
    if ($DeveloperBuild) { $updatedArgs += '-DeveloperBuild' }
    Invoke-InstallerProcess -OwnProcessTree -PreserveChildrenOnSuccess (Get-Process -Id $PID).Path $updatedArgs
    exit $LASTEXITCODE
}

$installLease = Enter-ContinuumInstallLease
try {
. (Join-Path $LibDir 'install-common.ps1')
Initialize-InstallEnvironment
if (-not $PrepareOnly) { Initialize-ElevationSession }
. (Join-Path $LibDir 'windows-service.ps1')
. (Join-Path $LibDir 'windows-prepared.ps1')
# A prior failed installer may have replaced an engine sealed by Active. Resume
# its fully verified Prepared candidate before any restaging can overwrite it.
# PrepareOnly remains non-activating; unrelated receipt damage still fails closed.
$recoverPrepared = $false
if (-not $PrepareOnly) { $recoverPrepared = $null -ne (Get-CoreDamagedSelectionRecovery) }
if ($ResumePrepared -or $recoverPrepared) {
    try { Resume-CorePreparedRelease -RepoRoot $RepoRoot -InstallLease $installLease }
    finally { Clear-Elevation }
    Write-Ok 'Prepared release is verified and supervised.'
    return
}
. (Join-Path $LibDir 'win-modules.ps1')
. (Join-Path $LibDir 'windows-prebuilt.ps1')

$WantsGrid = $Grid -or ($env:CONTINUUM_GRID -eq '1')

Write-Host ''
Write-Host '  Continuum installer (Windows, native)'
Write-Host '  -------------------------------------'
Write-Host "  Repo:  $RepoRoot"
if ($WantsGrid) { Write-Host '  Grid:  yes (GitHub login)' } else { Write-Host '  Grid:  no (local-only)' }
Write-Host ''

try {
    if (-not $PrepareOnly) {
    # Select storage before prerequisite downloads and extraction, not just cargo.
    Mod-ColdStorage
    $payloadRoot = Initialize-ManagedPayloadRoot -ColdRoot $env:CONTINUUM_STORAGE_PATH
    Write-Ok "installed payloads -> $payloadRoot"
    Test-WingetAvailable
    # Git + vendored submodules (llama.cpp, whisper.cpp) -- the native build needs
    # them. Per-user, no elevation.
    Install-IfMissing -Name 'Git' -WingetId 'Git.Git' `
        -TestCmd { Get-Command git -ErrorAction SilentlyContinue } -UserScope
    }
    if ($DeveloperBuild -and (Get-Command git -ErrorAction SilentlyContinue)) {
        Push-Location $RepoRoot
        try {
            Invoke-InstallerProcess -OwnProcessTree 'git' @('submodule', 'update', '--init', '--recursive')
            if ($LASTEXITCODE -ne 0) { throw 'Required submodule initialization failed; refusing to build an incomplete checkout.' }
        } finally { Pop-Location }
    }

    if (-not $DeveloperBuild) {
        # Refuse missing/incompatible publications before provisioning companions.
        $artifactDirectory = Get-CorePrebuiltRelease -RepoRoot $RepoRoot
    }
    if (-not $PrepareOnly) {
    # Toolchain. Per-user tools first (rustup -- no prompt); machine-scope tools
    # (VS Build Tools, CMake, LLVM, CUDA, gh) share the SINGLE gsudo UAC.
    if ($DeveloperBuild) {
    Mod-Rust
    Mod-VSBuildTools
    Mod-CMake
    Mod-LLVM
    Mod-CUDA
    }
    Mod-GhAuth -WantsGrid:$WantsGrid
    Mod-Airc
    Mod-OrtRuntime
    Mod-Poppler
    Mod-LiveKit -RepoRoot $RepoRoot

    # Grid transport reachability: Windows Firewall silently drops inbound peer
    # dials to the airc daemon unless it's allowed -- an asymmetric route failure
    # that breaks cross-grid delivery + peer-dialed inference. Grid-only; one gsudo
    # UAC (shared). A fresh grid box must not need a manual firewall click.
    Mod-AircFirewall -WantsGrid:$WantsGrid

    } else {
        Write-Step 'Preparing the release; provisioning, elevation, startup registration, and handoff are deferred.'
        if ($DeveloperBuild) {
        Mod-CMake -ExistingOnly
        Mod-LLVM -ExistingOnly
        Mod-CUDA -ExistingOnly
        }
    }

    # Build + run as the invoking user (never elevated -- keeps the cargo cache
    # user-owned so a later non-elevated `npm start` can rebuild).
    if ($DeveloperBuild) {
        Mod-BuildCore -RepoRoot $RepoRoot
        $release = New-CoreServiceRelease -RepoRoot $RepoRoot -ReconcileLegacyMedia:(-not $PrepareOnly)
    } else {
        $release = New-CoreServiceRelease -RepoRoot $RepoRoot -ArtifactDirectory $artifactDirectory -ReconcileLegacyMedia:(-not $PrepareOnly)
    }

    # Build llama-server.exe (the serving daemon's GPU-backend child) from the same
    # vendored llama.cpp. Windows twin of install-llama-server.sh. Without this the
    # serving daemon has no binary to spawn -> no local inference -> no persona can
    # speak. Needs CUDA + MSVC env (already provisioned above).
    if ($DeveloperBuild) {
        Mod-LlamaServer -RepoRoot $RepoRoot -InstallDirectory (Split-Path $release.engine)
    } else {
        Copy-CorePublishedEngine -RepoRoot $RepoRoot -ArtifactDirectory $artifactDirectory -InstallDirectory (Split-Path $release.engine)
    }

    Register-CoreServiceRelease -Release $release -RepoRoot $RepoRoot -PersistPreparedReceipt -PrepareOnly:$PrepareOnly
    if ($PrepareOnly) {
        Write-Ok 'Release prepared and validated. Deploy it with .\install.ps1 -ResumePrepared when ready for startup registration and handoff.'
        return
    }
    Invoke-CoreServiceRelease -Release $release -RepoRoot $RepoRoot -InstallLease $installLease
}
finally {
    # Always drop the cached elevation so an admin session never outlives install.
    Clear-Elevation
}

Write-Host ''
} finally { try { Clear-Elevation } finally { $installLease.Dispose() } }
Write-Ok 'Continuum native install complete.'
Write-Host '  Update: .\install.ps1 -Update  (fast-forward, fetch the published release, verify, and hand over)'
Write-Host '  Test:   continuum ping'
Write-Host ''

} # Installer entry: preserve diagnostics across hidden PowerShell process boundaries.
