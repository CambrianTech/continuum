# Windows: only the differences

Everything in [the install README](../README.md) applies. This page covers only what differs on Windows.

| | Windows |
|---|---|
| Platforms | `windows-x86_64` (`windows-arm64` when a CI leg exists) |
| Install root | `%LOCALAPPDATA%\Continuum` (generations, libraries); models under `%LOCALAPPDATA%\Continuum\models` or a drive chosen at install |
| Bootstrap | `bootstrap.ps1`: picks the binary, `Get-FileHash` verifies it. Runs in Windows PowerShell 5.1 (every Windows 10/11 has it) |
| Signed channel | A signed `.msi` that runs the same engine (no SmartScreen wall) |
| Elevation | One UAC, only for `admin` components: the S4U scheduled task that runs the core without a login, and the firewall rule for the grid port. One elevated broker runs all of them; there is no second prompt and no gsudo dependency |
| Supervisor | The `ContinuumCore` scheduled task, S4U, at boot, least privilege: one registrar (today there are three that disagree) |
| PATH | Prepends `%LOCALAPPDATA%\Continuum\bin` to the user PATH (`HKCU\Environment`) and to the current session |
| Library search | The engine starts the core with each backend's `lib\` directory on its DLL search path, never the system PATH |

## The supervisor's settings, and why

- **`ContinuumCore`.** S4U, RunLevel Limited, an AtStartup trigger, restart every minute up to 999
  times, no execution time limit, IgnoreNew for multiple instances, runs on battery. The user's SID gets
  read, write and execute on the task, so `continuum start` needs no elevation
  (`tools/scripts/register-core-service.ps1:43-62`).
- **Why S4U.** Until 2026-09-19 the core ran as an Interactive AtLogOn task. When that session ended,
  the 5090 went dark for 2 h 42 m, because RestartOnFailure never fires on session end
  (`register-core-service.ps1:32-42`).
- **One elevation registers every task.** Today a second task, `ContinuumDeploy` (every 10 minutes, same
  principal, 4 h limit), is registered by the same elevated run. Under this design the deploy job is the
  background update `Component` inside the core, so that second task is deleted.

## Windows facts the bootstrap and engine must respect

- **Path spelling.** Normalize extended path forms before comparing image paths: `\\?\C:\…` equals
  `C:\…`, and `\\?\UNC\srv\share` equals `\\srv\share`. CIM reports running images with the prefix,
  and a plain comparison once selected an occupied slot (`tools/scripts/lib/windows-prepared.ps1:3-14`).
- **Windows PowerShell 5.1 quirks.**
  - A PowerShell 7 parent can leak its PSModulePath into 5.1, which then fails at `Get-Acl`, so the
    bootstrap imports Management, Utility and Security from `$PSHOME` explicitly.
  - The hidden 5.1 console host drops `Write-Error`, so entry points write errors to stderr themselves.
  - Both are in `tools/scripts/lib/windows-elevation.ps1:7-25`.
- **Bounded downloads.** Downloads use HttpClient with a deadline and a size cap. PowerShell 5's
  `Invoke-WebRequest` hung about 4 minutes on a 10 KB file (2026-10-02;
  `tools/scripts/lib/win-modules.ps1:681-707`).
- **Choose the drive before downloading.** A Windows machine can have almost nothing free on C: and
  terabytes elsewhere: BIGGIEDESK had ~1.8 GB on C: and ~4.2 TB on D:. The measured placement in
  [the install README](../README.md#storage) decides, not a fixed drive.
- **Regression tests.** Until the engine replaces them, run
  `powershell -NoProfile -ExecutionPolicy RemoteSigned -File tools/scripts/tests/windows-service.test.ps1`
  (Windows PowerShell 5.1). It covers extended paths, occupied slots and mapped images.

## GPU

| Vendor | What the release ships | What the machine needs |
|---|---|---|
| NVIDIA | CUDA 12 and CUDA 13 runtime DLLs | A driver new enough for the shipped CUDA runtime. The floor is manifest data, derived from the runtime version. A missing or old driver is reported with NVIDIA's link; the engine does not install it |
| AMD / Intel | Vulkan (`lib\vulkan`) | The vendor's display driver, which carries the Vulkan driver |
| None | CPU | Nothing |

Today a non-NVIDIA Windows machine is refused the prebuilt engine. Under this design it gets the Vulkan
or CPU engine instead.
