//! The kernel's other extension slots, filled by their owners at boot (alongside
//! [`super::dispatch_hook`]): the vocabulary a command name resolves through, and what must
//! settle before a stop saves. The kernel names the contract; it never imports the owner.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Maps a caller's spelling of a command (a trained alias, a former name, a charset-legal
/// form) to its canonical name. Installed by the layer that owns that vocabulary.
pub type CommandNameResolver = fn(&str) -> String;

static NAME_RESOLVER: parking_lot::RwLock<Option<CommandNameResolver>> = parking_lot::RwLock::new(None);

/// Install the one command-name resolver. A second install replaces the first.
pub fn install_command_name_resolver(resolver: CommandNameResolver) {
    *NAME_RESOLVER.write() = Some(resolver);
}

/// The canonical name for `command`: through the installed resolver, or as given when none is
/// installed (a process that never booted the persona layer has no aliases to meet).
pub fn resolve_command_name(command: &str) -> String {
    match *NAME_RESOLVER.read() {
        Some(resolve) => resolve(command),
        None => command.to_owned(),
    }
}

/// Something that must finish before a stop saves (admitted turns, for one), given at most
/// `cap`. It reports its own outcome; the kernel only waits for it.
pub trait SettleBeforeStop: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn settle(&self, cap: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
}

static SETTLERS: parking_lot::RwLock<Vec<Arc<dyn SettleBeforeStop>>> = parking_lot::RwLock::new(Vec::new());

/// Install a participant; the same name twice keeps the first.
pub fn install_settle_before_stop(settler: Arc<dyn SettleBeforeStop>) {
    let mut settlers = SETTLERS.write();
    if settlers.iter().all(|s| s.name() != settler.name()) {
        settlers.push(settler);
    }
}

/// Let every participant settle, in install order, each within `cap`.
pub async fn settle_before_stop(cap: Duration) {
    let settlers: Vec<Arc<dyn SettleBeforeStop>> = SETTLERS.read().clone();
    for settler in settlers {
        settler.settle(cap).await;
    }
}
