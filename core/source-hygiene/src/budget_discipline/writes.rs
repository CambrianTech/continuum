//! `writes_outside_a_class`: a file written to a path its writer chose. Every directory the
//! substrate fills is a class with a claim; the class owner resolves the continuum home,
//! hands out the path and turns its grant into retention. A writer that picks its own path,
//! rotation count or age is how 460 GB filled the M5 (2026-07-13) and how ~15 writers grow
//! without bound today (inventory on canary 365f70fe4).

use super::{is_vendored, owned_by};
use crate::{sites, SourceFile, SourceRule, Violation};

/// The owners of persistent bytes: the ORM (stores), the disk classes, and continuum-pack's
/// stores (the content-addressed byte store every layer packs through; it writes only under the
/// root its caller hands it, so the class claim belongs to that caller, docs/architecture/PACKAGES.md).
const OWNERS: &[&str] = &[
    "core/continuum-kernel/src/orm/",
    "core/continuum-kernel/src/system_resources/disk_reporters",
    "core/continuum-pack/src/store/",
];

/// The ways Rust code creates or appends to a file.
const WRITES: &[&str] = &["fs::write(", "File::create(", "OpenOptions::new()"];

pub struct WritesOutsideAClass;

impl SourceRule for WritesOutsideAClass {
    fn name(&self) -> &'static str {
        "writes_outside_a_class"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if is_vendored(file) || owned_by(file, OWNERS) {
            return Vec::new();
        }
        sites(file, self.name(), |code| WRITES.iter().find(|w| code.contains(**w)).map(|w| format!("writes with `{w}`")))
    }
}
