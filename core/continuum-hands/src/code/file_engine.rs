//! File Engine — per-persona file operations with change tracking.
//!
//! Every write/edit/delete operation creates a ChangeNode in the change graph,
//! enabling undo at any point. Reads are side-effect-free.
//!
//! Thread safety: Each persona gets their own FileEngine instance.
//! The underlying ChangeGraph and PathSecurity handle concurrency.

use continuum_base::time::EpochMs;
use std::fs;
use std::path::{Path, PathBuf};

use uuid::Uuid;

use super::change_graph::ChangeGraph;
use super::diff_engine::compute_bidirectional_diff;
use super::path_security::{clean_path_arg, CleanedPathArg, PathSecurity, PathSecurityError, ReadScope};
use super::types::*;

/// Per-persona file engine with workspace scoping and change tracking.
pub struct FileEngine {
    persona_id: String,
    security: PathSecurity,
    graph: ChangeGraph,
    write_policy: WritePolicy,
}

/// What an edit that lands INSIDE a string literal should cost (#317).
///
/// Writing code as TEXT is a first-class thing a citizen does — a docstring example, a quoted
/// snippet, a fixture, a doc block. Talking about code is not a defect, and the gate must never
/// make her unable to do it. What she needs is to KNOW the text is not executing, which is why
/// the warning is UNCONDITIONAL and lives on every path (Joel, 2026-08-06: "They need to be able
/// to talk about code like any other first class citizen, just know that this isn't doing").
///
/// [`RefuseInert`](Self::RefuseInert) exists for the one context where that ambiguity does not
/// exist: a SCORED run, where the deliverable IS a patch that has to execute, graded by applying
/// the diff and running the tests. There, an inert insertion is never what she meant, the run
/// cannot recover from it (the verdict reads as a capability zero), and the file is the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritePolicy {
    /// Let the edit land and TELL her it is inert. Every live citizen path.
    #[default]
    Warn,
    /// Reject the edit, leave the file untouched, and say why — so the act→observe circuit
    /// turns it into a retry instead of a destroyed patch. Scored/benchmark paths only.
    RefuseInert,
}

/// Errors from file engine operations.
#[derive(Debug)]
pub enum FileEngineError {
    Security(PathSecurityError),
    Io(std::io::Error),
    NotFound(String),
    EditFailed(String),
}

impl std::fmt::Display for FileEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Security(e) => write!(f, "Security: {}", e),
            Self::Io(e) => write!(f, "I/O: {}", e),
            Self::NotFound(path) => write!(f, "File not found: {}", path),
            Self::EditFailed(msg) => write!(f, "Edit failed: {}", msg),
        }
    }
}

impl std::error::Error for FileEngineError {}

impl From<PathSecurityError> for FileEngineError {
    fn from(e: PathSecurityError) -> Self {
        match e {
            // In-sandbox ENOENT is not a security event and must never render with
            // the "Security:" prefix — that framing reads as FORBIDDEN and stops a
            // persona from correcting the path (the bitflags-exam 58-dead-reads bug).
            PathSecurityError::NotFound { .. } => Self::NotFound(e.to_string()),
            other => Self::Security(other),
        }
    }
}

impl From<std::io::Error> for FileEngineError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl FileEngine {
    /// A NotFound that tries to be useful — the same courtesy the executor
    /// already extends to a mistyped COMMAND name ("did you mean"), applied to a
    /// mistyped PATH. One constructor, used by every not-found site, so read,
    /// edit and write all recover the same way (compression: one decision, one
    /// place).
    fn not_found(&self, relative_path: &str) -> FileEngineError {
        let hits = suggest_nearest_paths(self.security.workspace_root(), relative_path);
        if hits.is_empty() {
            return FileEngineError::NotFound(relative_path.to_string());
        }
        FileEngineError::NotFound(format!(
            "{relative_path} — no such file. Did you mean: {}? (searched this \
             workspace; use file_tree/grep to look further)",
            hits.join(", ")
        ))
    }

    /// Create a new FileEngine for a persona.
    pub fn new(persona_id: &str, security: PathSecurity) -> Self {
        let workspace_id = format!("workspace-{}", persona_id);
        Self {
            persona_id: persona_id.to_string(),
            security,
            graph: ChangeGraph::new(&workspace_id),
            write_policy: WritePolicy::default(),
        }
    }

    /// Harden this engine for a SCORED run: an edit whose inserted code lands inside a string
    /// literal is refused rather than warned. See [`WritePolicy`] for why this is scoped to
    /// measurement and never applied to a living citizen's hands.
    pub fn with_write_policy(mut self, policy: WritePolicy) -> Self {
        self.write_policy = policy;
        self
    }

    /// The inert-edit stance this engine enforces.
    pub fn write_policy(&self) -> WritePolicy {
        self.write_policy
    }

}

mod apply;
mod edit_checks;
mod introspect;
mod ops;
mod page;
mod suggest;
use apply::*;
use edit_checks::*;
pub use edit_checks::{gutter_line_number, gutter_split};
pub use page::PageBudget;
use suggest::*;

#[cfg(test)]
mod tests;
