//! Whether a core handoff may proceed past the node's in-engine work (card 9beab2da).
//!
//! The windows 5090, 2026-10-28: the supported consumer adopted 84386a while the running core's
//! `serving/status` read `held_for` genome-train:8b46862c, and killed that in-engine run at 41%.
//! A handoff reads the hold first. This is the decision, pure, from what was observed; the CLI
//! gathers the observation.

/// What the running core's hold was observed to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldObservation {
    /// No core process is alive: nothing is held that a handoff could end.
    NoCore,
    /// The core answered and its hold names no live work.
    Free,
    /// The core answered and its hold names live work (its ids, as `ServingHold::live_work`
    /// says them).
    LiveWork(String),
    /// A core process is alive and its hold could not be read (a timeout, a refusal, a snapshot
    /// that does not decode). Not "nothing held": a busy training core answers slowly.
    Unreadable(String),
}

/// What closing in-engine training admission came to (card 91921d0f): done first, so the hold
/// read that follows is final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionClose {
    /// Closed until the handoff reopens it or the handoff process dies.
    Closed,
    /// The core has no serving lifecycle, or no core is running: it admits no in-engine training.
    NoServing,
    /// The running core predates `serving/handoff-close`: the hold read stands alone, and a run
    /// admitted after it is not prevented.
    NotSupported,
    /// A serving operation holds the gate now, so admission could not be closed.
    Busy,
    /// The close was asked and failed (a timeout, a refusal).
    Failed(String),
}

/// What the handoff does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldVerdict {
    /// Go on; when `Some`, say this first (what is known, or what `--force` is ending).
    Proceed(Option<String>),
    /// Wait: the consumer rechecks on its next pass with no attempt charged.
    Defer(String),
}

/// PURE: the verdict for a handoff that closed admission as `closed` and then read `observed`.
/// Only a confirmed absence of work, with admission closed (or nothing to close), proceeds on its
/// own; a close that is busy or failed defers like an unreadable hold, and a core that predates the
/// close proceeds on the hold read alone, saying the window stays open. `--force` (`force`)
/// proceeds through every deferral, saying what it ends or could not know.
pub fn verdict(closed: AdmissionClose, observed: HoldObservation, force: bool) -> HoldVerdict {
    let unclosed = match (&closed, &observed) {
        (_, HoldObservation::NoCore) => None,
        (AdmissionClose::Busy, _) => Some("a serving operation holds the lifecycle gate, so training admission could not be closed".to_string()),
        (AdmissionClose::Failed(why), _) => Some(format!("training admission could not be closed ({why})")),
        _ => None,
    };
    if let Some(why) = unclosed {
        return if force {
            HoldVerdict::Proceed(Some(format!("--force: {why}; this handoff proceeds while a run may still be admitted")))
        } else {
            HoldVerdict::Defer(format!("{why}. The deploy waits and retries on its next pass; `continuum reboot --force` overrides."))
        };
    }
    let held = hold_verdict(observed, force);
    match (closed, held) {
        (AdmissionClose::NotSupported, HoldVerdict::Proceed(said)) => {
            let window = "the running core predates serving/handoff-close, so a run admitted after this read is not prevented";
            HoldVerdict::Proceed(Some(said.map_or_else(|| window.to_string(), |said| format!("{said}; {window}"))))
        }
        (_, held) => held,
    }
}

/// PURE: the verdict on the hold read alone.
fn hold_verdict(observed: HoldObservation, force: bool) -> HoldVerdict {
    match (observed, force) {
        (HoldObservation::Free, _) => HoldVerdict::Proceed(None),
        (HoldObservation::NoCore, _) => HoldVerdict::Proceed(Some("no core process is running, so no live engine work is held".into())),
        (HoldObservation::LiveWork(work), true) => HoldVerdict::Proceed(Some(format!("--force: this handoff ends live engine work: {work}"))),
        (HoldObservation::LiveWork(work), false) => HoldVerdict::Defer(format!(
            "the engine is held for live work ({work}): a handoff now would end it. The deploy waits and retries on its next pass; `continuum reboot --force` ends the work instead."
        )),
        (HoldObservation::Unreadable(why), true) => HoldVerdict::Proceed(Some(format!(
            "--force: the running core's serving hold could not be read ({why}); this handoff proceeds without knowing what live work it ends"
        ))),
        (HoldObservation::Unreadable(why), false) => HoldVerdict::Defer(format!(
            "the running core is up but its serving hold could not be read ({why}), so what it carries is unknown. The deploy waits and retries on its next pass; `continuum reboot --force` overrides for a broken runtime."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex on #5551 at 6e6a4a1f0): a hold read that failed (a busy training
    // core answering slowly) collapsed into "nothing held", so the handoff killed the run anyway;
    // and `--force` skipped the read, ending work with no receipt of what it ended.
    #[test]
    fn only_a_confirmed_absence_of_work_proceeds_and_force_names_what_it_ends() {
        let work = || "genome-train:8b46862c on ggml-org/Qwen3.8-27B-GGUF (engine pid 37904, port 58057)".to_string();
        let closed = || AdmissionClose::Closed;
        assert_eq!(verdict(closed(), HoldObservation::Free, false), HoldVerdict::Proceed(None));
        assert!(matches!(verdict(AdmissionClose::NoServing, HoldObservation::NoCore, false), HoldVerdict::Proceed(Some(_))), "no core: said, then proceeds");
        assert!(matches!(verdict(closed(), HoldObservation::LiveWork(work()), false), HoldVerdict::Defer(why) if why.contains("genome-train:8b46862c")));
        assert!(matches!(verdict(closed(), HoldObservation::Unreadable("timed out".into()), false), HoldVerdict::Defer(why) if why.contains("timed out")),
            "an unreadable hold on a live core is unknown, never nothing");
        assert!(matches!(verdict(closed(), HoldObservation::LiveWork(work()), true), HoldVerdict::Proceed(Some(said)) if said.contains("genome-train:8b46862c")),
            "--force names the work it ends");
        assert!(matches!(verdict(closed(), HoldObservation::Unreadable("timed out".into()), true), HoldVerdict::Proceed(Some(said)) if said.contains("without knowing")));
    }

    // what this catches (card 91921d0f, Codex on #5551): a run admitted between the hold read and
    // the stop. A free hold proceeds only once admission is closed: a busy gate or a failed close
    // defers (forceable, said); a core that predates the close proceeds on the read alone and says
    // the window stays open; with no core there is nothing to close.
    #[test]
    fn a_free_hold_proceeds_only_with_admission_closed_or_nothing_to_close() {
        assert_eq!(verdict(AdmissionClose::NoServing, HoldObservation::Free, false), HoldVerdict::Proceed(None), "no serving lifecycle: nothing to admit");
        assert!(matches!(verdict(AdmissionClose::Busy, HoldObservation::Free, false), HoldVerdict::Defer(why) if why.contains("lifecycle gate")));
        assert!(matches!(verdict(AdmissionClose::Failed("timed out".into()), HoldObservation::Free, false), HoldVerdict::Defer(why) if why.contains("timed out")));
        assert!(matches!(verdict(AdmissionClose::Busy, HoldObservation::Free, true), HoldVerdict::Proceed(Some(said)) if said.contains("--force")));
        assert!(matches!(verdict(AdmissionClose::NotSupported, HoldObservation::Free, false), HoldVerdict::Proceed(Some(said)) if said.contains("predates")));
        assert!(matches!(verdict(AdmissionClose::NotSupported, HoldObservation::LiveWork("w".into()), false), HoldVerdict::Defer(_)), "live work still defers on an old core");
        assert!(matches!(verdict(AdmissionClose::Busy, HoldObservation::NoCore, false), HoldVerdict::Proceed(Some(_))), "no core: nothing held, nothing to close");
    }
}
