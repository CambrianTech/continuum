//! `DeployBuildConsumer` — the warm build's seat at the resource-governor table.
//!
//! A node that deploys itself compiles its next core beside the serving one (the warm
//! build). That build is a real, recurring consumer of memory, but nothing held a place
//! for it: the serving plan sized itself to the whole budget, and a warm build then found
//! no room and was refused. On the M5, 2026-09-27, the lane #4464 relaunched just after a
//! build had released its memory was sized in that surplus (6 slots × 30,976, an 11.5 GB
//! host cache, 31.4 GB resident) and held it; free memory sat at 8-10 GiB, under any build
//! floor, and every deploy was refused while it ran a stale core under a day of merges.
//!
//! So the build reserves its measured floor
//! ([`WARM_BUILD_MIN_FREE_BYTES`](crate::inference::llama_server::WARM_BUILD_MIN_FREE_BYTES):
//! the first rustc job plus the reserve kept for the citizens) as a standing reservation,
//! the same way the embed lane does, so the serving plan plans around it. While a build
//! runs, this consumer reports what the build's processes actually hold, and the ledger
//! withholds only the part of the floor they do not already hold (#4481), so a build is
//! never counted twice.
//!
//! The build cannot be paused or shrunk from here: a reclaim is refused, named.

use async_trait::async_trait;

use crate::resources::{ConsumerFootprint, ReclaimOutcome, ReclaimRequest, ResourceConsumer, ResourceKind};

/// Stable id for the warm build's reservation and footprint on the board.
pub const DEPLOY_BUILD_CONSUMER_ID: &str = "deploy-build";

/// The processes a warm build is made of: cargo and the compilers and linkers it runs
/// (rustc for the core, a C/C++ toolchain and cmake for the engine).
const BUILD_PROCESS_NAMES: &[&str] =
    &["cargo", "rustc", "cc", "c++", "clang", "clang++", "cc1", "cc1plus", "ld", "ld64", "cmake", "make", "ninja"];

/// PURE: whether a process belongs to a warm build of this node: a build tool whose
/// command line names one of the build's roots (the deploy tree or the shared cargo
/// target directory). A build of some other checkout is not the deploy build.
pub fn is_deploy_build_process(name: &str, cmdline: &str, roots: &[String]) -> bool {
    let name = name.trim_end_matches(".exe");
    BUILD_PROCESS_NAMES.contains(&name) && roots.iter().any(|root| !root.is_empty() && cmdline.contains(root.as_str()))
}

/// The memory kind the serving plan budgets with on this node: one pool on unified memory
/// and on CPU serving (the plan's budget is the governed `Vram` figure there), system RAM
/// beside a discrete card (the build does not touch the card).
pub fn reserve_kind() -> ResourceKind {
    match crate::gpu::monitor::detect().map(|m| m.memory_mode()) {
        Some(crate::gpu::monitor::MemoryMode::Discrete) => ResourceKind::Ram,
        _ => ResourceKind::Vram,
    }
}

pub struct DeployBuildConsumer {
    kind: ResourceKind,
    roots: Vec<String>,
}

impl DeployBuildConsumer {
    /// `roots`: the paths a deploy build's command lines name (the deploy tree, the cargo
    /// target directory).
    pub fn new(kind: ResourceKind, roots: Vec<String>) -> Self {
        Self { kind, roots }
    }

    /// The deploy tree this core runs from: the nearest ancestor of its working directory
    /// holding the build definition (`tools/scripts/start-server.sh`). `None` on a node
    /// with no source tree (an installed release): it never warm-builds, and reserves
    /// nothing for a build.
    pub fn deploy_tree() -> Option<std::path::PathBuf> {
        let cwd = std::env::current_dir().ok()?.canonicalize().ok()?;
        cwd.ancestors()
            .find(|d| d.join("tools/scripts/start-server.sh").is_file())
            .map(std::path::Path::to_path_buf)
    }

    /// The deploy tree and the shared cargo target directory.
    pub fn roots_for(tree: &std::path::Path) -> Vec<String> {
        let mut roots = vec![tree.to_string_lossy().into_owned()];
        let target = std::env::var("CARGO_TARGET_DIR")
            .ok()
            .filter(|t| !t.is_empty())
            .or_else(|| dirs::home_dir().map(|h| h.join(".continuum/cache/cargo-target").to_string_lossy().into_owned()));
        roots.extend(target);
        roots
    }

    /// Reserve the warm build's floor and take a seat on the board, on a node that builds
    /// itself. Idempotent, like the embed lane's floor.
    pub fn register(daemon: &crate::resources::ResourceDaemon) {
        let Some(tree) = Self::deploy_tree() else {
            crate::probe!(
                class = "deploy.build.reserve_skipped",
                "no deploy tree under this core's working directory: it never warm-builds, nothing reserved"
            );
            return;
        };
        let kind = reserve_kind();
        let floor = crate::inference::llama_server::WARM_BUILD_MIN_FREE_BYTES;
        daemon.reserve(DEPLOY_BUILD_CONSUMER_ID, kind, floor);
        daemon.add_consumer(std::sync::Arc::new(Self::new(kind, Self::roots_for(&tree))));
        crate::probe!(
            class = "deploy.build.reserved",
            kind = format!("{kind:?}").as_str(),
            bytes = floor,
            tree = %tree.display(),
            "the warm build reserved its floor: serving plans around the room a deploy needs"
        );
    }

    fn resident_bytes(&self) -> u64 {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
        let mut sys = System::new();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_memory().with_cmd(UpdateKind::Always),
        );
        sys.processes()
            .values()
            .filter(|p| {
                let cmd = p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
                is_deploy_build_process(&p.name().to_string_lossy(), &cmd, &self.roots)
            })
            .map(|p| p.memory())
            .fold(0u64, |acc, b| acc.saturating_add(b))
    }
}

#[async_trait]
impl ResourceConsumer for DeployBuildConsumer {
    fn consumer_id(&self) -> &str {
        DEPLOY_BUILD_CONSUMER_ID
    }

    fn footprint(&self) -> Vec<ConsumerFootprint> {
        let bytes = self.resident_bytes();
        if bytes == 0 {
            return Vec::new(); // no build running: the reservation alone holds the room
        }
        vec![ConsumerFootprint { kind: self.kind, bytes, detail: "warm build (cargo, rustc, the engine's toolchain)".to_string() }]
    }

    async fn reclaim(&self, _request: ReclaimRequest) -> ReclaimOutcome {
        ReclaimOutcome::refused(
            "a warm build cannot be paused or shrunk from here; it releases everything when it ends".to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a deploy build that the board cannot see (so its floor is counted
    // on top of its own residency), or someone else's build counted as the deploy's (so the
    // deploy's floor reads met when it is not). Only build tools whose command line names
    // the deploy tree or the shared target directory are the deploy build.
    #[test]
    fn only_build_tools_working_on_the_deploy_roots_are_the_deploy_build() {
        let roots = vec!["/Users/joel/Development/continuum".to_string(), "/Users/joel/.continuum/cache/cargo-target".to_string()];
        assert!(is_deploy_build_process(
            "rustc",
            "rustc --crate-name continuum_core --out-dir /Users/joel/.continuum/cache/cargo-target/release/deps",
            &roots
        ));
        assert!(is_deploy_build_process("cargo", "cargo build --manifest-path /Users/joel/Development/continuum/core/continuum-core/Cargo.toml", &roots));
        assert!(is_deploy_build_process("rustc.exe", "rustc --out-dir /Users/joel/.continuum/cache/cargo-target/x", &roots));
        assert!(!is_deploy_build_process("rustc", "rustc --out-dir /tmp/other-project/target", &roots), "another checkout's build");
        assert!(!is_deploy_build_process("llama-server", "llama-server -m /Users/joel/Development/continuum/x.gguf", &roots), "not a build tool");
        assert!(!is_deploy_build_process("rustc", "rustc anything", &[String::new()]), "an empty root matches nothing");
    }
}
