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

/// Let every participant settle, in install order, all within ONE `cap`: each is given what
/// the ones before it left. The stop's caller waits for `cap` and the save after it, never for
/// a cap per participant (the M5, 2026-10-27 22:20Z: the turns took the whole 300 s, the page
/// checkpoint then ran past the CLI's 315 s wait, and the core was killed before it saved).
pub async fn settle_before_stop(cap: Duration) {
    let settlers: Vec<Arc<dyn SettleBeforeStop>> = SETTLERS.read().iter().map(|(_, s)| Arc::clone(s)).collect();
    settle_within(&settlers, cap).await;
}

async fn settle_within(settlers: &[Arc<dyn SettleBeforeStop>], cap: Duration) {
    let deadline = tokio::time::Instant::now() + cap;
    for settler in settlers {
        settler.settle(deadline.saturating_duration_since(tokio::time::Instant::now())).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A participant that records the cap it was given and spends `takes` of it.
    struct Spends {
        takes: Duration,
        given: Arc<parking_lot::Mutex<Vec<Duration>>>,
    }

    impl SettleBeforeStop for Spends {
        fn name(&self) -> &'static str {
            "spends"
        }
        fn settle(&self, cap: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
            self.given.lock().push(cap);
            let takes = self.takes.min(cap);
            Box::pin(async move { tokio::time::sleep(takes).await })
        }
    }

    // what this catches: each participant handed the whole cap, so the stop outlives the wait
    // its caller gives it (the M5, 2026-10-27 22:20Z).
    #[tokio::test(start_paused = true)]
    async fn participants_share_one_stop_budget() {
        let given = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let settlers: Vec<Arc<dyn SettleBeforeStop>> = vec![
            Arc::new(Spends { takes: Duration::from_secs(250), given: given.clone() }),
            Arc::new(Spends { takes: Duration::from_secs(250), given: given.clone() }),
            Arc::new(Spends { takes: Duration::from_secs(250), given: given.clone() }),
        ];
        let started = tokio::time::Instant::now();
        settle_within(&settlers, Duration::from_secs(300)).await;
        assert!(started.elapsed() <= Duration::from_secs(301), "the whole stop fits its one cap");
        let given = given.lock().clone();
        assert_eq!(given[0], Duration::from_secs(300));
        assert!(given[1] <= Duration::from_secs(50), "the second gets what the first left: {:?}", given[1]);
        assert_eq!(given[2], Duration::ZERO, "nothing left for the third");
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
