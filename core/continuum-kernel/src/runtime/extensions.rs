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
    /// Its description, for logs. Identity is the participant's type.
    fn name(&self) -> &'static str;
    fn settle(&self, cap: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
}

static SETTLERS: parking_lot::RwLock<Vec<(std::any::TypeId, Arc<dyn SettleBeforeStop>)>> =
    parking_lot::RwLock::new(Vec::new());

/// Install a participant, identified by its type; a second install of the same type keeps the
/// first.
pub fn install_settle_before_stop<S: SettleBeforeStop>(settler: Arc<S>) {
    let id = std::any::TypeId::of::<S>();
    let mut settlers = SETTLERS.write();
    if settlers.iter().all(|(installed, _)| *installed != id) {
        settlers.push((id, settler));
    }
}

/// Let every participant settle, in install order, each within `cap`.
pub async fn settle_before_stop(cap: Duration) {
    let settlers: Vec<Arc<dyn SettleBeforeStop>> = SETTLERS.read().iter().map(|(_, s)| Arc::clone(s)).collect();
    for settler in settlers {
        settler.settle(cap).await;
    }
}

/// The citizens this node hosts, by id, as the persona layer knows them; `None` when no
/// roster is up (the boot window, a test, a core without personas). Installed at boot.
pub type RosterProvider = fn() -> Option<Vec<uuid::Uuid>>;

static ROSTER: parking_lot::RwLock<Option<RosterProvider>> = parking_lot::RwLock::new(None);

/// Install the one roster provider.
pub fn install_roster(provider: RosterProvider) {
    *ROSTER.write() = Some(provider);
}

/// The hosted citizens, or `None` when nothing installed a roster or it is not up yet.
pub fn roster() -> Option<Vec<uuid::Uuid>> {
    (*ROSTER.read()).and_then(|provider| provider())
}

/// How many citizen turns are running now. Installed at boot by the layer that admits turns.
pub type TurnsInFlight = fn() -> u64;

static TURNS_IN_FLIGHT: parking_lot::RwLock<Option<TurnsInFlight>> = parking_lot::RwLock::new(None);

/// Install the one turns-in-flight count.
pub fn install_turns_in_flight(count: TurnsInFlight) {
    *TURNS_IN_FLIGHT.write() = Some(count);
}

/// Citizen turns running now; `None` when nothing installed the count, so a caller that must
/// know (a quiescence gate) refuses rather than reading an unknown as zero.
pub fn turns_in_flight() -> Option<u64> {
    (*TURNS_IN_FLIGHT.read()).map(|count| count())
}
