//! What a mind that is not a citizen left behind is reclaimed.
//!
//! The citizen registry decides who exists: a persona is a citizen while her seed is on disk
//! (`citizens/personas/<name>/seed.json`, dormant ones included). Her per-mind state lives in
//! directories named by her id: her mind dir (`personas/<id>`, `perception_feed::mind_dir`) and
//! her saved tool results (the spill root, `tool_executor::spill::spill_root`). Nothing removed
//! those for minds that never became citizens or no longer are: on the M5 (2026-11-14) 7,432 mind
//! dirs (4.5 GB) and 7,504 spill dirs (984 MB, back to July) stood beside 16 citizens, and every
//! pass over them (the disk pool's walk each broker tick) grew with the pile.
//!
//! It is a [`ServiceModule`](crate::runtime::ServiceModule) like every other owner with a life:
//! its `initialize` runs the pass on a blocking thread and the runtime initializes modules before it
//! admits any command, so no `persona/spawn` can mint a home while the pass reads the registry.
//!
//! One pass at boot, off the async workers, removes every id-named directory under those roots
//! whose id is not a citizen AND that holds no memory. Memory is never reclaimed: `memory/*` maps
//! any UUID to a store under `personas/<id>`, so agents (Codex, the memory bridge) keep memory
//! there without a persona seed. A directory not named by an id is not a mind's and is left alone.
//! An empty citizen set is refused: it means the registry could not be read, never that no one
//! exists. Archived citizens (`citizens/personas-archive*`) are citizens: archives are kept.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use uuid::Uuid;

/// What one reclaim pass removed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reclaimed {
    pub dirs: usize,
    pub failed: usize,
}

/// Why a reclaim pass did not run.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReclaimRefusal {
    #[error("the citizen registry read no citizens: nothing is reclaimed against an empty registry")]
    NoCitizens,
}

/// Reclaim, under each of `roots`, every directory named by an id that is not in `citizens` and
/// for which `holds_memory` is false.
pub fn reclaim(
    roots: &[PathBuf],
    citizens: &HashSet<Uuid>,
    holds_memory: impl Fn(Uuid) -> bool,
) -> Result<Reclaimed, ReclaimRefusal> {
    if citizens.is_empty() {
        return Err(ReclaimRefusal::NoCitizens);
    }
    let mut reclaimed = Reclaimed::default();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else { continue }; // an absent root holds no minds
        for entry in entries.flatten() {
            let Some(id) = entry.file_name().to_str().and_then(|name| Uuid::parse_str(name).ok()) else {
                continue;
            };
            if citizens.contains(&id) || !entry.path().is_dir() || holds_memory(id) {
                continue;
            }
            match std::fs::remove_dir_all(entry.path()) {
                Ok(()) => reclaimed.dirs += 1,
                Err(error) => {
                    reclaimed.failed += 1;
                    continuum_base::probe!(
                        class = "persona.orphan_reclaim_failed",
                        path = %entry.path().display(),
                        error = %error
                    );
                }
            }
        }
    }
    Ok(reclaimed)
}

/// The owner of minds' per-mind directories on this node.
pub struct MindHomesModule {
    continuum_root: PathBuf,
}

impl MindHomesModule {
    pub fn new(continuum_root: PathBuf) -> Self {
        Self { continuum_root }
    }
}

#[async_trait::async_trait]
impl crate::runtime::ServiceModule for MindHomesModule {
    fn config(&self) -> crate::runtime::ModuleConfig {
        crate::runtime::ModuleConfig {
            name: "mind_homes",
            priority: crate::runtime::ModulePriority::Background,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 1,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        let root = self.continuum_root.clone();
        if let Err(error) = tokio::task::spawn_blocking(move || reclaim_at_boot(&root)).await {
            continuum_base::probe!(class = "persona.orphan_reclaim_panicked", error = %error);
        }
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        _params: serde_json::Value,
    ) -> Result<crate::runtime::CommandResult, String> {
        // The runtime routes no command here (no command_prefixes); the trait still asks.
        Err(String::from(command))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The boot pass: the citizens are the seeds under `continuum_root`, the roots are every
/// per-mind root this node keeps.
fn reclaim_at_boot(continuum_root: &Path) {
    // Every persona seed the registry holds, live, dormant or archived: each sibling of the
    // persona kind dir under `citizens/` is read the same way (a dir with no persona seeds adds none).
    let persona_dir =
        crate::identity::citizen_path::citizens_kind_dir(continuum_root, crate::identity::IdentityKind::Persona);
    let registry: Vec<PathBuf> = persona_dir
        .parent()
        .and_then(|citizens| std::fs::read_dir(citizens).ok())
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default(); // unwrap_or: an unreadable registry yields no citizens, which reclaim refuses
    let citizens: HashSet<Uuid> =
        registry.iter().flat_map(|dir| crate::persona::seed::persona_ids_with_seeds_in(dir)).collect();
    let mut roots: Vec<PathBuf> = crate::persona::perception_feed::mind_dir(Uuid::nil())
        .and_then(|dir| dir.parent().map(Path::to_path_buf))
        .into_iter()
        .collect();
    if let Ok(spills) = crate::cognition::tool_executor::spill::spill_root() {
        roots.push(spills);
    }
    match reclaim(&roots, &citizens, crate::modules::data::DataState::holds_citizen_store) {
        Ok(reclaimed) => continuum_base::probe!(
            class = "persona.orphans_reclaimed",
            citizens = citizens.len() as u64,
            dirs = reclaimed.dirs as u64,
            failed = reclaimed.failed as u64,
            "per-mind directories of minds that are not citizens removed"
        ),
        Err(refusal) => continuum_base::probe!(class = "persona.orphan_reclaim_refused", reason = %refusal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: per-mind directories of non-citizens piling up forever (7,432 on the
    /// M5), without ever taking memory. A non-citizen's id dir is removed; a citizen's, one holding
    /// memory (an agent's), a non-id dir and a plain file stay; an empty citizen set reclaims nothing.
    #[test]
    fn only_non_citizen_mind_dirs_are_reclaimed() {
        let root = tempfile::tempdir().unwrap();
        let citizen = Uuid::new_v4();
        let orphan = Uuid::new_v4();
        let agent_memory = Uuid::new_v4();
        for name in [citizen.to_string(), orphan.to_string(), agent_memory.to_string(), "general".to_string()] {
            std::fs::create_dir_all(root.path().join(&name).join("deep")).unwrap();
        }
        std::fs::write(root.path().join(Uuid::new_v4().to_string()), b"a file").unwrap();
        let roots = vec![root.path().to_path_buf()];

        let holds_memory = |id: Uuid| id == agent_memory;
        assert_eq!(reclaim(&roots, &HashSet::new(), holds_memory), Err(ReclaimRefusal::NoCitizens));
        assert!(root.path().join(orphan.to_string()).exists(), "an empty registry reclaims nothing");

        let reclaimed = reclaim(&roots, &HashSet::from([citizen]), holds_memory).unwrap();
        assert_eq!(reclaimed, Reclaimed { dirs: 1, failed: 0 });
        assert!(!root.path().join(orphan.to_string()).exists());
        assert!(root.path().join(citizen.to_string()).exists());
        assert!(root.path().join(agent_memory.to_string()).exists(), "memory is never reclaimed");
        assert!(root.path().join("general").exists());
    }
}
