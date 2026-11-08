/// Windows handoffs have lost their engines (Codex: RemovedDead, windows-b87bdce5b4, cause
/// unresolved), and the Linux service unit sets no `KillMode`, so systemd may end the whole cgroup
/// with the core. Neither is known to keep a lane until a fixture on that platform proves it.
pub(super) const KEEPS_ENGINE_LANES: bool = false;
