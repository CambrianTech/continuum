//! The teardown DECISION, separated from the OS that carries it out.
//!
//! Ending a core this caller has no handle on is two problems: deciding WHICH process
//! may be ended and in WHAT ORDER, and actually ending it. The second needs Windows.
//! The first is arithmetic over strings, and it is where every defect has been — a
//! recycled pid, a plan swapped in `%TEMP%`, a drain that ran before the capability was
//! held. Those live here so they can be exercised on any machine, in seconds.
//!
//! Measured 2026-09-22 on the Windows node: `OpenProcess(25040, TERMINATE)` returned
//! NULL with Win32 error 5, and so did the same call asking only for
//! `QUERY_LIMITED_INFORMATION`. The core runs under the supervisor's S4U principal; the
//! CLI does not. There is no permission to grant to the caller, so the process can only
//! be ended by a principal that holds the privilege — borrowed through the installer's
//! existing consent boundary, with the plan's SHA-256 carried on the RunAs argv because
//! a same-user process can rewrite a file in `%TEMP%` between the write and the consent
//! but not the argv of a process already spawned (#4232).
//!
//! WHY A PID IS NOT THE PLAN. A pid is reusable. The plan names the pid AND the
//! installation directory, and the consented child proves — through the one handle it
//! will terminate with — that the process is a core of ours living there.
//!
//! There is no `stop --escalate` for an operator to discover. The product contract is
//! that the same command works every time (`continuum install`), and a repair verb
//! would be a second thing to know about on the node whose operator does not know it
//! exists.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeardownPlan {
    pub pid: i32,
    /// The directory the installed core lives in, from the `ContinuumCore` descriptor.
    /// A DIRECTORY and not a file, because the unelevated half cannot see which image
    /// the process is actually executing — see `target_is_our_core`.
    pub install_dir: String,
}

/// `stop`'s options.
///
/// Plain `stop` is unchanged and is the whole user-facing surface. `--elevated --plan
/// --plan-sha` IS the consented child of an escalation and does nothing else — it is
/// not a verb anyone types, it is how this binary re-enters itself through
/// `Start-Process -Verb RunAs`, the same three-flag shape `install --supervisor` uses.
///
/// There is deliberately NO `--escalate` for an operator to find: the user-facing
/// contract is one command, `continuum install`, which reaches this boundary itself
/// when it has to. A separate repair verb would be a
/// second thing to know about, and the node that needs it is the one whose operator
/// does not know it exists.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct StopOptions {
    pub elevated: bool,
    pub plan: Option<String>,
    pub plan_sha: Option<String>,
}

impl StopOptions {
    pub fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--elevated" => options.elevated = true,
                "--plan" => options.plan = args.next(),
                "--plan-sha" => options.plan_sha = args.next(),
                other => return Err(format!("stop: unknown option {other}")),
            }
        }
        // The three go together or none do: a child without its digest could act on any
        // file in %TEMP%, and a digest without a child is a flag with nothing to bind.
        if options.elevated != options.plan.is_some() || options.elevated != options.plan_sha.is_some()
        {
            return Err(
                "stop: --elevated, --plan and --plan-sha go together (the consented child ends \
                 exactly one process, bound by its digest)"
                    .to_string(),
            );
        }
        Ok(options)
    }
}

/// The receipt the elevated child leaves beside the plan. The parent reads THIS, never
/// the child's console — an elevated child's stdout is a window that closes.
pub fn digest(bytes: &[u8]) -> String {
    crate::supervisor_install::plan_digest(bytes)
}

/// The plan the consent was given for, or a refusal.
///
/// Pure, so the one gate between "a file in `%TEMP%`" and "an elevated terminate" is
/// pinned by a test on every platform, not only where it runs.
pub fn bind_plan_bytes(
    bytes: &[u8],
    plan_sha: &str,
    shown_as: &Path,
) -> Result<TeardownPlan, String> {
    let actual = digest(bytes);
    if !actual.eq_ignore_ascii_case(plan_sha) {
        return Err(format!(
            "the teardown plan at {} is not the one the consent was given for \
             (sha {} on argv, {actual} on disk) — refusing to terminate anything",
            shown_as.display(),
            plan_sha.get(..12).unwrap_or(plan_sha)
        ));
    }
    serde_json::from_slice(bytes).map_err(|e| format!("the plan is not a TeardownPlan: {e}"))
}

/// Is the process this handle names one of OUR cores?
///
/// WHY THIS IS NOT AN EXPECTED-IMAGE EQUALITY.   The unelevated half cannot open the process — that is the
/// whole reason it is escalating — so any image it puts in the plan is a GUESS, taken
/// from the `ContinuumCore` descriptor's `artifact` field. On the node this rail exists
/// for, that guess is wrong in a specific and predictable way: the descriptor names
/// `service-b/core.exe` (the new 108b slot) while pid 25040 is executing the RENAMED
/// predecessor. Gating on equality with a guess does not fail closed, it fails ALWAYS,
/// and precisely on the broken state we are trying to repair.
///
/// So the plan carries what the planner can actually know — the installation directory,
/// a stable fact from the same descriptor — and the child proves, through the handle it
/// will terminate with, that the running image is a core of ours living there. A pid
/// recycled into `notepad.exe` fails the directory; a pid recycled into `llama-server`
/// (which DOES live in that directory) fails the name. The exact image and its digest
/// are then reported in the receipt, so what was killed is recorded rather than assumed.
///
/// Pure over what was OBSERVED, so the rule is testable without a process to kill.
/// Paths compare the way Windows means them — case-insensitively, and `/` spelled `\`
/// is the same separator — because the scheduler and `QueryFullProcessImageName`
/// disagree about spelling far more often than they disagree about the file.
pub fn target_is_our_core(plan: &TeardownPlan, observed_image: &str) -> Result<(), String> {
    let norm = |s: &str| s.replace('/', "\\").trim_matches('"').to_ascii_lowercase();
    let root = norm(&plan.install_dir);
    let root = root.trim_end_matches('\\').to_string();
    let image = norm(observed_image);
    if !image.starts_with(&format!("{root}\\")) {
        return Err(format!(
            "pid {} is running {observed_image}, which is not under the installation \
             directory {} the consent named — the pid was recycled; refusing to terminate it",
            plan.pid, plan.install_dir
        ));
    }
    let name = image.rsplit('\\').next().unwrap_or(&image);
    if !CORE_IMAGE_NAMES.contains(&name) {
        return Err(format!(
            "pid {} is running {observed_image}, which is in the installation directory but \
             is not a core image ({}) — refusing to terminate it",
            plan.pid,
            CORE_IMAGE_NAMES.join(", ")
        ));
    }
    Ok(())
}

/// The file names a continuum core can legitimately be executing from. `.prev.exe` is
/// here deliberately: a core that outlived its own deploy is running from the parking
/// space, and that is the single most likely state at the moment this rail is used.
const CORE_IMAGE_NAMES: &[&str] = &[
    "core.exe",
    "core.prev.exe",
    "continuum-core-server.exe",
    "continuum-core-server.prev.exe",
];

/// SHA-256 of a file on disk, read in one shot. Images are tens of megabytes; this is
/// not a hot path and it runs at most twice per escalation.
pub trait HeldCapability: Sized {
    /// The image the HELD process is running. Read through the capability, never by a
    /// second lookup on the pid.
    fn image(&self) -> Result<String, String>;
    /// Spend it. Consumes self: a capability is used once.
    fn spend(self) -> Result<(), String>;
}

/// The teardown sequence, generic over the capability so the order is testable without
/// an OS to kill a process on.
///
/// EVERY STEP BEFORE `drain` MUST BE ABLE TO FAIL WITH NOTHING DRAINED. That is the
/// whole contract — a declined or failed consent must not strand a drained core — and it
/// is why `drain` is the fourth argument and not the second.
pub async fn teardown_sequence<C, T, F, Fut>(
    plan: &TeardownPlan,
    take: T,
    drain: F,
) -> Result<(String, String), String>
where
    C: HeldCapability,
    T: FnOnce(i32) -> Result<C, String>,
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = String>,
{
    // THE CAPABILITY COMES FIRST, and everything after it is read THROUGH it. Taking it
    // kills nothing; what it buys is that the pid stops being a name two different
    // processes could answer to between the check and the act.
    let held = take(plan.pid)?;
    let image = held.image()?;
    target_is_our_core(plan, &image)?;
    // Only now, with the capability in hand and the target proven, may anything be
    // drained. A failure above this line leaves a serving core serving.
    let graceful = drain().await;
    held.spend()?;
    Ok((image, graceful))
}

pub fn elevation_outcome(
    pid: i32,
    consent: Result<String, String>,
    receipt_text: &str,
) -> Result<(), String> {
    let Err(why) = consent else { return Ok(()) };
    if why.contains("canceled by the user") {
        return Err(format!(
            "install: the elevation consent was refused — pid {pid} is untouched and still \
             serving. Nothing was drained, nothing was terminated"
        ));
    }
    Err(format!(
        "install: the elevated teardown did not complete: {why}\n  receipt: {}",
        if receipt_text.trim().is_empty() {
            "(none — the child never ran, or ran and could not write one)"
        } else {
            receipt_text.trim()
        }
    ))
}

/// The unelevated half: write the plan, ask for ONE consent, read the receipt. Whether
/// the process actually WENT is proven by the caller on the same bounded deadline the
/// ordinary teardown uses — an immediate liveness probe after a terminate sees a process
/// mid-exit, which is the mistake this file already made once.
#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> TeardownPlan {
        TeardownPlan {
            pid: 25040,
            install_dir: "C:\\Users\\a\\.continuum\\bin".to_string(),
        }
    }

    // what this catches: a consented child that is not bound to a plan. --elevated
    // without --plan-sha would terminate whatever the file in %TEMP% currently names,
    // which is the entire attack the digest exists to close.
    #[test]
    fn the_consented_child_cannot_be_invoked_unbound() {
        let parse = |a: &[&str]| StopOptions::parse(a.iter().map(|s| s.to_string()));
        assert_eq!(parse(&[]).expect("plain stop"), StopOptions::default());

        let unbound = parse(&["--elevated", "--plan", "p.json"])
            .expect_err("a child without its digest must be refused");
        assert!(unbound.contains("go together"), "{unbound}");
        let stray = parse(&["--plan-sha", "abc"])
            .expect_err("a digest with no child is a flag bound to nothing");
        assert!(stray.contains("go together"), "{stray}");
        let unknown = parse(&["--escalate"])
            .expect_err("there is no operator-facing escalate verb — `continuum install` is the one command");
        assert!(unknown.contains("unknown option"), "{unknown}");

        let child = parse(&["--elevated", "--plan", "p.json", "--plan-sha", "abc"])
            .expect("the bound child parses");
        assert_eq!(child.plan.as_deref(), Some("p.json"));
        assert_eq!(child.plan_sha.as_deref(), Some("abc"));
    }

    // what this catches (#4232, carried to this module): a plan read
    // from %TEMP% that is not the plan the human approved. Any same-user process can
    // rewrite that file between the write and the consent; it cannot rewrite the argv
    // of a process already spawned. Without the digest gate, the consent for "terminate
    // the core" registers as consent for "terminate whatever this file now names".
    #[test]
    fn only_the_bytes_the_consent_was_given_for_can_be_acted_on() {
        let bytes = serde_json::to_vec_pretty(&plan()).expect("a plan serializes");
        let sha = digest(&bytes);
        let bound = bind_plan_bytes(&bytes, &sha, Path::new("plan.json"))
            .expect("the planned bytes bind to their own digest");
        assert_eq!(bound, plan());

        let mut swapped = plan();
        swapped.pid = 4;
        let swapped_bytes = serde_json::to_vec_pretty(&swapped).expect("a plan serializes");
        let refused = bind_plan_bytes(&swapped_bytes, &sha, Path::new("plan.json"))
            .expect_err("a rewritten plan must not bind to the approved digest");
        assert!(
            refused.contains("refusing to terminate anything"),
            "the refusal must say nothing was killed: {refused}"
        );
    }

    // what this catches: splitting the drain and the terminate across the consent
    // boundary, which strands a drained core when the consent is declined. Every
    // check the child makes before the drain — the plan digest, and the identity of the
    // process behind the handle — must be able to fail with NOTHING drained, which is
    // only true while they are refusals rather than partial work. This pins that each
    // returns an error naming what it refused, so a reordering that drains first would
    // have to delete an assertion rather than merely pass.
    #[test]
    fn every_pre_capability_check_refuses_instead_of_half_acting() {
        let p = plan();
        let bytes = serde_json::to_vec_pretty(&p).expect("a plan serializes");
        for (what, refusal) in [
            (
                "plan digest",
                bind_plan_bytes(&bytes, &"0".repeat(64), Path::new("plan.json")).unwrap_err(),
            ),
            (
                "outside the installation",
                target_is_our_core(&p, "C:\\Windows\\System32\\notepad.exe").unwrap_err(),
            ),
            (
                "inside but not a core",
                target_is_our_core(&p, "C:\\Users\\a\\.continuum\\bin\\llama-server.exe")
                    .unwrap_err(),
            ),
        ] {
            assert!(
                refusal.to_lowercase().contains("refusing"),
                "the {what} check must REFUSE, so nothing has been drained when it fires: {refusal}"
            );
        }
    }

    // what this catches: an elevated terminate on a RECYCLED pid. The number alone is not
    // identity — between writing the plan and the human answering the prompt, the core
    // can exit and the OS can hand 25040 to anything. Two ways that goes wrong and both
    // are refused: something else entirely, and something that merely LIVES in our
    // installation directory (llama-server does, and killing it would take the lane).
    #[test]
    fn a_recycled_pid_is_refused_rather_than_terminated() {
        let p = plan();
        let elsewhere = "C:\\Windows\\System32\\notepad.exe";
        let refused = target_is_our_core(&p, elsewhere)
            .expect_err("a process outside the installation must be refused");
        assert!(refused.contains("recycled"), "the reason must name reuse: {refused}");
        assert!(refused.contains(elsewhere), "the refusal names what is there: {refused}");

        let neighbour = "C:\\Users\\a\\.continuum\\bin\\llama-server.exe";
        let sibling = target_is_our_core(&p, neighbour)
            .expect_err("our own engine is not our core; terminating it would take the lane");
        assert!(sibling.contains("not a core image"), "{sibling}");

        // A path that merely STARTS with the directory's characters is not inside it.
        let lookalike = "C:\\Users\\a\\.continuum\\bin-evil\\core.exe";
        assert!(
            target_is_our_core(&p, lookalike).is_err(),
            "a sibling directory sharing a prefix is not the installation directory"
        );
    }

    // what this catches, and it is the case that made this gate an installation-directory
    // proof instead of an expected-image equality: the descriptor
    // names the NEW slot while the surviving core still executes the RENAMED predecessor.
    // An equality check against the planner's guess would refuse here — failing not
    // closed but ALWAYS, and exactly on the broken state this rail exists to repair.
    #[test]
    fn a_core_running_from_the_renamed_predecessor_is_still_our_core() {
        let p = plan();
        target_is_our_core(&p, "C:\\Users\\a\\.continuum\\bin\\core.prev.exe")
            .expect("a core that outlived its own deploy runs from the parking space");
        target_is_our_core(&p, "C:\\Users\\a\\.continuum\\bin\\core.exe")
            .expect("and the ordinary case still passes");
    }

    // A capability whose every act is RECORDED, so the sequence can be asserted rather
    // than described. `spend` consumes it, which is the type system carrying the
    // once-only property the real handle has.
    #[derive(Default)]
    struct Recorder {
        log: std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>,
        image: String,
    }

    impl HeldCapability for Recorder {
        fn image(&self) -> Result<String, String> {
            self.log.borrow_mut().push("image");
            Ok(self.image.clone())
        }
        fn spend(self) -> Result<(), String> {
            self.log.borrow_mut().push("spend");
            Ok(())
        }
    }

    // what this catches: ORCHESTRATION, not helpers. The contract is that NOTHING IS DRAINED until the
    // capability is in hand and the target is proven — a declined consent, a token
    // without the privilege, or a recycled pid must each leave a serving core serving.
    // Until the drain was injected this could only be asserted in a comment; now a fake
    // capability records the order and the assertion is the order itself.
    #[tokio::test]
    async fn nothing_is_drained_before_the_capability_is_held_and_the_target_proven() {
        let p = plan();
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

        // The happy path: take, observe THROUGH what was taken, drain, spend.
        let l = log.clone();
        let taken = l.clone();
        // Built BEFORE the closure: the sequence borrows the plan, so the closure must
        // not also reach into it.
        let ours = format!("{}\\core.exe", p.install_dir);
        let (image, graceful) = teardown_sequence(
            &p,
            move |_pid| {
                taken.borrow_mut().push("take");
                Ok(Recorder { log: taken.clone(), image: ours.clone() })
            },
            || {
                l.borrow_mut().push("drain");
                async { "durable".to_string() }
            },
        )
        .await
        .expect("a held capability on a proven target completes");
        assert_eq!(
            log.borrow().as_slice(),
            &["take", "image", "drain", "spend"],
            "the drain runs AFTER the capability is held and the target proven, and the \
             capability is spent only after the drain"
        );
        assert!(image.ends_with("core.exe"));
        assert_eq!(graceful, "durable");

        // A capability that cannot be TAKEN: nothing observed, nothing drained. This is
        // the declined-consent and no-privilege case, and it is the one that would
        // strand a drained core if the order were wrong.
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let l = log.clone();
        let refused = teardown_sequence::<Recorder, _, _, _>(
            &p,
            |_pid| Err("OpenProcess denied".to_string()),
            || {
                l.borrow_mut().push("drain");
                async { String::new() }
            },
        )
        .await
        .expect_err("no capability must abort before anything is drained");
        assert!(refused.contains("OpenProcess denied"), "{refused}");
        assert!(log.borrow().is_empty(), "NOTHING ran: {:?}", log.borrow());

        // A RECYCLED pid: the capability was taken, but what it names is not our core.
        // The target check sits before the drain precisely so this leaves the world alone.
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let l = log.clone();
        let taken = l.clone();
        let wrong = teardown_sequence(
            &p,
            move |_pid| {
                taken.borrow_mut().push("take");
                Ok(Recorder {
                    log: taken.clone(),
                    image: "C:\\Windows\\System32\\notepad.exe".to_string(),
                })
            },
            || {
                l.borrow_mut().push("drain");
                async { String::new() }
            },
        )
        .await
        .expect_err("a recycled pid must abort before anything is drained");
        assert!(wrong.contains("recycled"), "{wrong}");
        assert_eq!(
            log.borrow().as_slice(),
            &["take", "image"],
            "it looked, it refused, and it neither drained nor spent: {:?}",
            log.borrow()
        );
    }

    // what this catches (measured on the Windows node 2026-09-22): the core is ALREADY
    // DRAINED — ingress closed at 21:33:09, gate deliberately not reopening — and the
    // teardown runs against it anyway. The sequence must still terminate, and the
    // receipt must carry the drain's own words VERBATIM rather than flattening them,
    // because "drained (nothing to drain)" and "drained (Durable)" are the difference
    // between a core that saved just now and one that saved forty minutes ago.
    #[tokio::test]
    async fn an_already_drained_core_is_still_terminated_and_the_receipt_says_so() {
        let p = plan();
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let l = log.clone();
        let taken = l.clone();
        let ours = format!("{}\\core.exe", p.install_dir);
        let (image, graceful) = teardown_sequence(
            &p,
            move |_pid| {
                taken.borrow_mut().push("take");
                Ok(Recorder { log: taken.clone(), image: ours.clone() })
            },
            || {
                l.borrow_mut().push("drain");
                // What an already-drained core answers: nothing left to do.
                async { "NothingRunning".to_string() }
            },
        )
        .await
        .expect("an already-drained core is still a core that has to go");

        assert_eq!(
            log.borrow().as_slice(),
            &["take", "image", "drain", "spend"],
            "the order does not change because the drain was a no-op: {:?}",
            log.borrow()
        );
        assert_eq!(
            graceful, "NothingRunning",
            "the drain's own answer is carried through, not normalised into success"
        );
        assert!(image.ends_with("core.exe"));
    }

    // what this catches: a REFUSED ELEVATION read as a failed teardown. The operator
    // declined, the child never ran, and the core is exactly as it was — if that reads
    // as "the teardown failed" the next person hunts for damage that does not exist.
    // The other arm is the opposite risk: an elevation that DID run and left no receipt
    // must never be reported as if nothing happened.
    #[test]
    fn a_refused_consent_says_nothing_was_touched_and_a_silent_failure_does_not() {
        use super::elevation_outcome;

        elevation_outcome(25040, Ok("ok".to_string()), "terminated pid 25040")
            .expect("a completed elevation is not an error");

        let refused = elevation_outcome(
            25040,
            Err("Start-Process: The operation was canceled by the user.".to_string()),
            "",
        )
        .expect_err("a declined consent must not read as success");
        assert!(refused.contains("untouched and still serving"), "{refused}");
        assert!(
            refused.contains("Nothing was drained, nothing was terminated"),
            "the refusal must say what did NOT happen, in those words: {refused}"
        );

        let silent = elevation_outcome(25040, Err("exit code 1".to_string()), "   ")
            .expect_err("a failed elevation is an error");
        assert!(
            silent.contains("the child never ran, or ran and could not write one"),
            "an absent receipt is reported as absent, never as nothing-happened: {silent}"
        );
        assert!(
            !silent.contains("untouched"),
            "only a DECLINED consent may claim the target is untouched: {silent}"
        );

        let with_receipt =
            elevation_outcome(25040, Err("exit code 1".to_string()), "elevated teardown failed: X")
                .expect_err("still an error");
        assert!(with_receipt.contains("elevated teardown failed: X"), "{with_receipt}");
    }

    // what this catches: path spelling read as a different file. The scheduler echoes
    // `Execute` as registered while QueryFullProcessImageName answers with the OS's own
    // spelling; treating `C:/x/Core.exe` and `c:\x\core.exe` as different targets would
    // refuse every legitimate teardown on some machines and none of the dangerous ones.
    #[test]
    fn windows_path_spelling_is_not_a_different_target() {
        let p = plan();
        target_is_our_core(&p, "c:/Users/a/.continuum/BIN/CONTINUUM-CORE-SERVER.EXE")
            .expect("case and separator spelling name the same place on Windows");
    }
}
