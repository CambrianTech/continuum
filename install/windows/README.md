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

## GPU

| Vendor | What the release ships | What the machine needs |
|---|---|---|
| NVIDIA | CUDA 12 and CUDA 13 runtime DLLs | Driver 551.61 or later. A missing driver is reported with NVIDIA's link; the engine does not install it |
| AMD / Intel | Vulkan (`lib\vulkan`) | The vendor's display driver, which carries the Vulkan driver |
| None | CPU | Nothing |

Today a non-NVIDIA Windows machine is refused the prebuilt engine. Under this design it gets the Vulkan
or CPU engine instead.
