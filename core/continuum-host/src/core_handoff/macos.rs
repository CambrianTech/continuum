/// macOS: the core stops alone (`kill_pid_alone`, never a group kill: pid 61259 RemovedDead on the
/// M5 and the IntelMac before that fix), and the launchd job abandons its process group
/// (`AbandonProcessGroup`, which the plist drift check enforces), so the live engine serves on
/// through every reboot and the next core adopts it.
pub(super) const KEEPS_ENGINE_LANES: bool = true;
