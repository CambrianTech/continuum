# Shared Windows installer elevation implementation. No application install is required.
# Imported by Continuum; standalone artifact boundary for AIRC integration.
# After editing the launcher, run tools/scripts/sync-windows-bootstrap.ps1.
param([System.Collections.IDictionary]$GsudoSource)
$script:ElevationGsudoSource = $GsudoSource

function Initialize-InstallerPowerShell {
    # A PS7 desktop host may pass its PSModulePath to Windows PowerShell 5.
    # Load the running engine's built-ins explicitly before autoload can select
    # another engine's Security/Utility type data. Keep user module paths intact.
    foreach ($name in @('Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility', 'Microsoft.PowerShell.Security')) {
        $manifest = [IO.Path]::Combine($PSHOME, 'Modules', $name, ($name + '.psd1'))
        Import-Module $manifest -Global -ErrorAction Stop
    }
}
Initialize-InstallerPowerShell

# Only executable installer entries serialize errors to the OS pipe. Library
# consumers keep normal PowerShell ErrorRecord/redirection semantics. PS5's
# hidden console host can otherwise discard Write-Error before its native caller
# can capture it, even though the underlying process stderr was drained.
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
# Native background commands must never allocate a console when the caller is
# a desktop harness. Keep both pipes draining and preserve the native exit code.
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
    if ($OwnProcessTree -and -not ('Continuum.Setup.OwnedProcess' -as [type])) {
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
public sealed class OwnedProcess : IDisposable {
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
    public static OwnedProcess Start(ProcessStartInfo start) {
        var owned = new OwnedProcess();
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
        if ($OwnProcessTree) { $process = [Continuum.Setup.OwnedProcess]::Start($start) }
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

function Update-SessionPath {
    # Keep tools selected in this installer session, then discover newly
    # registered tools. Repeated refreshes must not grow PATH past Windows' limit.
    $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    $paths = New-Object 'System.Collections.Generic.List[string]'
    foreach ($source in @($env:PATH, [Environment]::GetEnvironmentVariable('PATH', 'User'), [Environment]::GetEnvironmentVariable('PATH', 'Machine'))) {
        foreach ($path in ($source -split ';')) {
            if (-not [string]::IsNullOrWhiteSpace($path) -and $seen.Add($path)) { $paths.Add($path) }
        }
    }
    $env:PATH = $paths -join ';'
}

function Test-IsAdmin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    return (New-Object Security.Principal.WindowsPrincipal($id)).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator)
}

# One cache is bound to the outer installer's process and inherited by children.
# No global cache, persistent gsudo setting, or application installation is needed.
$script:ElevationWarmed = $false
$script:InstallElevationSession = $null
$script:GsudoExecutable = $null

function Initialize-ElevationSession {
    if ($script:InstallElevationSession) { return }
    $inherited = $env:CAMBRIAN_INSTALL_ELEVATION
    if ($inherited) {
        try {
            $context = $inherited | ConvertFrom-Json -ErrorAction Stop
            $ownerId = [int]$context.ownerPid
            if ($context.version -ne 1 -or $ownerId -le 0) { throw 'Invalid owner context.' }
            $owner = Get-Process -Id $ownerId -ErrorAction Stop
            if ($owner.StartTime.ToUniversalTime().Ticks.ToString() -ne $context.ownerStarted) {
                throw 'The installer owner process has changed.'
            }
            # Do not let stale/unrelated environment data widen the allowed tree.
            $ancestorId = $PID
            $seen = @{}
            while ($ancestorId -ne $ownerId) {
                if ($ancestorId -le 0 -or $seen.ContainsKey($ancestorId)) { throw 'Owner is not an ancestor.' }
                $seen[$ancestorId] = $true
                $ancestor = Get-CimInstance Win32_Process -Filter "ProcessId = $ancestorId" -ErrorAction Stop
                if (-not $ancestor) { throw 'Cannot resolve installer ancestry.' }
                $ancestorId = [int]$ancestor.ParentProcessId
            }
        } catch { throw "Cannot borrow installer elevation: $($_.Exception.Message)" }
        $script:InstallElevationSession = [pscustomobject]@{ OwnerPid = $ownerId; Borrowed = $true; ExistingCache = [bool]$context.existingCache }
    } else {
        $script:GsudoExecutable = Find-GsudoExecutable
        $existingCache = $false
        if ($script:GsudoExecutable -and -not (Test-IsAdmin)) { $existingCache = Test-ElevationCacheAvailable }
        $context = [ordered]@{
            version = 1
            existingCache = $existingCache
            ownerPid = $PID
            ownerStarted = (Get-Process -Id $PID).StartTime.ToUniversalTime().Ticks.ToString()
        }
        $env:CAMBRIAN_INSTALL_ELEVATION = $context | ConvertTo-Json -Compress
        $script:InstallElevationSession = [pscustomobject]@{ OwnerPid = $PID; Borrowed = $false; ExistingCache = $existingCache }
    }
}

function Find-GsudoExecutable {
    # An alias, PowerShell function or Git Bash wrapper can break cache ancestry.
    $command = Get-Command gsudo.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($command -and [IO.Path]::IsPathRooted($command.Source) -and (Test-Path -LiteralPath $command.Source -PathType Leaf)) { return $command.Source }
}

# Probe only; never starts or extends someone else's credential cache.
function Test-ElevationCacheAvailable {
    $savedErrorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $PSNativeCommandUseErrorActionPreference = $false
        $answer = @(Invoke-InstallerProcess $script:GsudoExecutable @('status', 'CacheAvailable') 2>&1)
        $code = $global:LASTEXITCODE
    } finally { $ErrorActionPreference = $savedErrorPreference }
    $value = ($answer -join [Environment]::NewLine).Trim()
    if ($code -eq 0 -and $value -eq 'true') { return $true }
    if ($code -eq 1 -and $value -eq 'false') { return $false }
    throw "Cannot inspect the existing elevation cache (exit $code): $value"
}

function Ensure-Gsudo {
    $script:GsudoExecutable = Find-GsudoExecutable
    if ($script:GsudoExecutable) { return }
    $source = $script:ElevationGsudoSource
    if (-not $source -or $source.type -ne 'winget' -or -not $source.id -or $source.scope -ne 'user') {
        throw 'The shared installer manifest must supply a per-user gsudo package source.'
    }
    Write-Host 'Installing gsudo (per-user) -- the shared elevation helper ...'
    Invoke-InstallerProcess -OwnProcessTree 'winget' @('install', '--id', $source.id, '--source', 'winget', '--exact', '--silent',
        '--accept-package-agreements', '--accept-source-agreements', '--scope', $source.scope)
    $code = $global:LASTEXITCODE
    if ($code -ne 0 -and $code -ne 3010) { throw "gsudo acquisition failed (winget exit $code)." }
    Update-SessionPath
    $script:GsudoExecutable = Find-GsudoExecutable
    if (-not $script:GsudoExecutable) { throw 'gsudo acquisition completed but the native executable is unavailable after PATH refresh.' }
}

# First actual admin operation acquires the cache, even if it is in a child.
# -p restricts authorization to the owner process and descendants. -d -1 avoids
# the default five-minute idle expiry during builds; owner finally/process exit
# ends this session. Never use pid 0, persistent CacheMode Auto, or global -k.
function Ensure-Elevated {
    param([string]$Reason = 'the current installer operation')
    Initialize-ElevationSession
    if ($script:InstallElevationSession.ExistingCache) {
        if (-not $script:GsudoExecutable) { Ensure-Gsudo }
        if (-not (Test-ElevationCacheAvailable)) { throw 'The pre-existing elevation cache expired; refusing to acquire a replacement without a new installer run.' }
        $script:ElevationWarmed = $true
        return
    }
    if ($script:ElevationWarmed) { return }
    if (Test-IsAdmin) { $script:ElevationWarmed = $true; return }
    Ensure-Gsudo
    Write-Host "Admin access needed for $Reason -- acquiring or reusing the installer elevation session."
    Write-Host 'gsudo is a third-party elevation helper. Windows may show its publisher in the consent prompt.'
    Write-Host 'Approval covers installer admin steps; authentication, builds and the core stay unelevated.'
    $savedErrorPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $PSNativeCommandUseErrorActionPreference = $false
        $diagnostic = @(Invoke-InstallerProcess $script:GsudoExecutable @('cache', 'on', '-p', $script:InstallElevationSession.OwnerPid, '-d', '-1') 2>&1)
        $code = $global:LASTEXITCODE
    } finally { $ErrorActionPreference = $savedErrorPreference }
    if ($code -ne 0) {
        $detail = ($diagnostic | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
        if ([string]::IsNullOrWhiteSpace($detail)) { $detail = 'gsudo returned no diagnostic output.' }
        throw "Elevation failed while $Reason (gsudo cache on exit $code).$([Environment]::NewLine)$detail"
    }
    $script:ElevationWarmed = $true
}

function Clear-Elevation {
    if (-not $script:InstallElevationSession) { return }
    if (-not $script:InstallElevationSession.Borrowed -and -not $script:InstallElevationSession.ExistingCache -and -not (Test-IsAdmin)) {
        # A child may have acquired the cache before the parent needed elevation.
        # Locate an installed helper without provisioning anything during cleanup.
        if (-not $script:GsudoExecutable) {
            $script:GsudoExecutable = Find-GsudoExecutable
            if (-not $script:GsudoExecutable) {
                Update-SessionPath
                $script:GsudoExecutable = Find-GsudoExecutable
            }
        }
        if ($script:GsudoExecutable) {
            $savedErrorPreference = $ErrorActionPreference
            try {
                $ErrorActionPreference = 'Continue'
                $PSNativeCommandUseErrorActionPreference = $false
                $diagnostic = @(Invoke-InstallerProcess $script:GsudoExecutable @('cache', 'off', '-p', $script:InstallElevationSession.OwnerPid) 2>&1)
                $code = $global:LASTEXITCODE
            } finally { $ErrorActionPreference = $savedErrorPreference }
            if ($code -ne 0) { throw "Elevation cache cleanup failed (exit $code): $($diagnostic -join [Environment]::NewLine)" }
        } elseif ($script:ElevationWarmed) {
            throw 'Cannot close the installer elevation session: the native gsudo executable is missing.'
        }
    }
    if (-not $script:InstallElevationSession.Borrowed) { Remove-Item Env:CAMBRIAN_INSTALL_ELEVATION -ErrorAction SilentlyContinue }
    $script:InstallElevationSession = $null
    $script:ElevationWarmed = $false
}

function Invoke-Elevated {
    param([Parameter(Mandatory = $true)][ValidateNotNullOrEmpty()][string[]]$CommandLine,
        [string]$Reason = 'running the elevated installer command')
    if (Test-IsAdmin) {
        $arguments = @($CommandLine | Select-Object -Skip 1)
        Invoke-InstallerProcess $CommandLine[0] $arguments
        return
    }
    Ensure-Elevated -Reason $Reason
    Invoke-InstallerProcess $script:GsudoExecutable $CommandLine
}
