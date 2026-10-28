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

/// What the handoff does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldVerdict {
    /// Go on; when `Some`, say this first (what is known, or what `--force` is ending).
    Proceed(Option<String>),
    /// Wait: the consumer rechecks on its next pass with no attempt charged.
    Defer(String),
}

/// PURE: the verdict for `observed`. Only a confirmed absence of work proceeds on its own; live
/// work and an unreadable hold on a live core defer, and `--force` (`force`) proceeds through
/// either, naming what it ends or that it could not know.
pub fn verdict(observed: HoldObservation, force: bool) -> HoldVerdict {
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
        assert_eq!(verdict(HoldObservation::Free, false), HoldVerdict::Proceed(None));
        assert!(matches!(verdict(HoldObservation::NoCore, false), HoldVerdict::Proceed(Some(_))), "no core: said, then proceeds");
        assert!(matches!(verdict(HoldObservation::LiveWork(work()), false), HoldVerdict::Defer(why) if why.contains("genome-train:8b46862c")));
        assert!(matches!(verdict(HoldObservation::Unreadable("timed out".into()), false), HoldVerdict::Defer(why) if why.contains("timed out")),
            "an unreadable hold on a live core is unknown, never nothing");
        assert!(matches!(verdict(HoldObservation::LiveWork(work()), true), HoldVerdict::Proceed(Some(said)) if said.contains("genome-train:8b46862c")),
            "--force names the work it ends");
        assert!(matches!(verdict(HoldObservation::Unreadable("timed out".into()), true), HoldVerdict::Proceed(Some(said)) if said.contains("without knowing")));
    }
}
