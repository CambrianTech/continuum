//! The governed serving budget a lane sizes against, as a slot the engine owns: the
//! serving daemon (`modules::serving_daemon::governed_host_budget`) computes it from the
//! memory authority's board and installs it at boot. A mind sizing an ephemeral lane (an
//! exam, a single-purpose lane) reads it here and never reaches into the module layer.

use crate::resources::ResourceDaemon;

/// Usable serving bytes on this host under the governed budget, read from `daemon`.
pub type GovernedUsableBytes = fn(&ResourceDaemon) -> u64;

static USABLE: parking_lot::RwLock<Option<GovernedUsableBytes>> = parking_lot::RwLock::new(None);

/// Install the one budget reader. A second install replaces the first.
pub fn install(reader: GovernedUsableBytes) {
    *USABLE.write() = Some(reader);
}

/// Usable serving bytes now: `None` when no memory authority is up or no serving daemon
/// installed the reader (a test, a tool), which a caller reads as "unknown", never zero.
pub fn usable_bytes() -> Option<u64> {
    let reader = (*USABLE.read())?;
    crate::resources::ResourceDaemon::global().map(|daemon| reader(&daemon))
}
