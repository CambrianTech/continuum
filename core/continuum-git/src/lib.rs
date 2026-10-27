//! Git's working-tree residue and a deploy checkout's submodules, judged once. See
//! [`index_lock`] and [`submodules`].

pub mod index_lock;
pub mod submodules;

/// Tests that spawn `git` and tests that judge whether a `git` is live in a tree run one at a
/// time. The live-git judgment counts a git whose working directory cannot be read as possibly
/// working in the tree (an exiting process reads that way), so a git spawned by a neighbouring
/// test made `index_lock`'s dead-lock test fail in CI (2026-10-27, run 40512737561).
#[cfg(test)]
pub(crate) static GIT_PROCESSES: std::sync::Mutex<()> = std::sync::Mutex::new(());
