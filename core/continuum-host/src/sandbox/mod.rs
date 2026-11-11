//! A child confined to an allow-list of filesystem roots: the contract once, and one adapter per
//! OS for the mechanism (macOS seatbelt, Linux landlock, none elsewhere, said on every spawn). The
//! policy, which roots a caller may reach, is continuum-kernel's `system_resources::confinement`.

use std::path::{Path, PathBuf};

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), path = "other.rs")]
mod os;
mod profile;

pub use profile::seatbelt_profile;

/// Credential files that sit wherever their scope does, not at one home path: every
/// airc project scope keeps its agent's signing key at `<repo>/.airc/identity.key`
/// (the deploy checkout's is an agent's), and a resident who read one could sign as
/// that agent. Matched by trailing path components: the file verbs check the suffix,
/// seatbelt denies it by regex. Landlock cannot express a pattern; no Linux node hosts
/// residents today.
pub const CREDENTIAL_SUFFIXES: &[&str] = &[".airc/identity.key"];

/// Whether `path` ends in a [`CREDENTIAL_SUFFIXES`] entry. One check for every consumer.
pub fn is_scoped_credential(path: &Path) -> bool {
    CREDENTIAL_SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

/// What the file verbs and the enforcers agree a citizen may touch: absolute,
/// canonical roots. Produced once per spawn by the policy (continuum-kernel `Confinement::materialize`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowList {
    /// Roots she may read AND write (her workspace, her citizen dir, caches, temp).
    pub write: Vec<PathBuf>,
    /// Roots she may read only (dotfiles, `~/.continuum` minus the exclusions).
    pub read: Vec<PathBuf>,
    /// Credential files carved OUT of a granted root (the policy's carve-outs).
    /// The last word everywhere: a deny-capable enforcer writes them after every
    /// grant; an allow-only enforcer never sees them, because a granted root that
    /// contains one is already replaced by its children minus the carve-out
    /// (the policy's carve). Another citizen's directory needs no carve-out: it is never
    /// granted (the citizens root is enumerated without `peers`).
    pub deny: Vec<PathBuf>,
    /// Directories a carve-out was cut out of: listable themselves (a `ls ~` works), with
    /// nothing beneath them granted by this entry. Their granted children are in `write`
    /// or `read`.
    pub listable: Vec<PathBuf>,
}

impl AllowList {
    /// Whether `path` (absolute, canonical) is readable under this list: inside a
    /// granted root and not a carve-out. A carve-out is the last word, write root or
    /// not (`~/.cache/huggingface/token` sits inside a write root).
    pub fn permits_read(&self, path: &Path) -> bool {
        !self.deny.iter().any(|d| path.starts_with(d))
            && !is_scoped_credential(path)
            && self.write.iter().chain(self.read.iter()).any(|r| path.starts_with(r))
    }
}

/// What the enforcer did for one spawn: the receipt every spawn probe carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enforcement {
    /// The child is confined by the named mechanism.
    Enforced { mechanism: &'static str },
    /// This host cannot confine a child at this level; the child runs as the operator
    /// and the reason is on the probe. Never silent.
    Unavailable { reason: String },
}

impl Enforcement {
    pub fn is_enforced(&self) -> bool {
        matches!(self, Self::Enforced { .. })
    }
}

/// `program` with `args`, confined to `list` under `home` by this OS's mechanism, and the receipt
/// of what was enforced.
pub fn confine(home: &Path, list: &AllowList, program: &Path, args: &[std::ffi::OsString]) -> (tokio::process::Command, Enforcement) {
    os::confine(home, list, program, args)
}
