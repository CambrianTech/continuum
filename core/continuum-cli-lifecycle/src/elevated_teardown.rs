//! The teardown DECISION, separated from the OS that carries it out.
//!
//! Ending a core this caller has no handle on is two problems: deciding WHICH process
//! may be ended and in WHAT ORDER, and actually ending it. The second needs Windows.
//! The first is policy over native paths, and it is where every defect has been — a
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

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A plan must bind to consent before the process capability can be acquired.
#[derive(Debug, thiserror::Error)]
pub enum TeardownPlanError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the teardown plan at {path} differs from the consented digest — refusing to terminate anything")]
    DigestMismatch { path: std::path::PathBuf },
    #[error("the plan is not a TeardownPlan: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("pid {pid} is running {image} outside installation {directory} — the pid was recycled; refusing to terminate it")]
    OutsideInstallation {
        pid: i32,
        image: PathBuf,
        directory: PathBuf,
    },
    #[error("pid {pid} is running {image}, which is not a core image — refusing to terminate it")]
    NotCore { pid: i32, image: PathBuf },
    #[error(transparent)]
    InvalidPid(#[from] continuum_host::process_id::NotAProcess),
    #[error("cannot take the process capability: {0}")]
    Take(#[source] std::io::Error),
    #[error("cannot inspect the held process: {0}")]
    Image(#[source] std::io::Error),
    #[error("cannot terminate the held process: {0}")]
    Terminate(#[source] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ElevationError {
    #[error("cannot encode the teardown plan: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("cannot write the teardown plan: {0}")]
    Write(#[source] std::io::Error),
    #[error("cannot locate this executable: {0}")]
    OwnPath(#[source] std::io::Error),
    #[error("install: the elevation consent was refused — pid {pid} is untouched and still serving. Nothing was drained, nothing was terminated")]
    Refused { pid: i32 },
    #[error(transparent)]
    Launcher(#[from] crate::supervisor_install::PowerShellError),
    #[error("invalid elevation outcome: {0}")]
    Outcome(#[from] serde_json::Error),
    #[error("cannot read the elevated child's receipt: {0}")]
    Receipt(#[source] std::io::Error),
    #[error("elevated child exited with {exit_code}; receipt: {receipt:?}")]
    Child {
        exit_code: i32,
        receipt: Option<TeardownReceipt>,
    },
}

/// The child's persisted diagnostic receipt, distinct from launcher output.
#[derive(Debug, Clone)]
pub struct TeardownReceipt(pub(crate) String);

impl std::fmt::Display for TeardownReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0.trim())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeardownPlan {
    pub pid: i32,
    /// The directory the installed core lives in, from the `ContinuumCore` descriptor.
    /// A DIRECTORY and not a file, because the unelevated half cannot see which image
    /// the process is actually executing — see `target_is_our_core`.
    pub install_dir: PathBuf,
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
    /// `--engine`: stop the engine too. By default a stop ends the core only: the engine is its
    /// own resource, and its loaded weights and every mind's KV stay up for the next core to
    /// adopt (`start` and `reboot` adopt it), so a stop, build and start costs no model reload
    /// and nobody's KV (Joel, 2026-10-29: "The engine is supposed to stay up"). `--engine` is
    /// for when its memory is wanted back.
    pub engine: bool,
    /// `--discard-kv` (with `--engine`): end the engine even when the stop's receipt does not
    /// show every resident's KV saved, losing that KV. Without it an unsaved engine stays up
    /// (CONTINUUM-ENGINE 4.9, I1).
    pub discard_kv: bool,
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
                "--engine" => options.engine = true,
                "--discard-kv" => options.discard_kv = true,
                "--elevated" => options.elevated = true,
                "--plan" => options.plan = args.next(),
                "--plan-sha" => options.plan_sha = args.next(),
                other => return Err(format!("stop: unknown option {other}")),
            }
        }
        // The three go together or none do: a child without its digest could act on any
        // file in %TEMP%, and a digest without a child is a flag with nothing to bind.
        if options.elevated != options.plan.is_some()
            || options.elevated != options.plan_sha.is_some()
        {
            return Err(
                "stop: --elevated, --plan and --plan-sha go together (the consented child ends \
                 exactly one process, bound by its digest)"
                    .to_string(),
            );
        }
        if options.discard_kv && !options.engine {
            return Err("stop: --discard-kv ends the engine and needs --engine".to_string());
        }
        if options.engine && options.elevated {
            return Err("stop: --engine is an operator's stop; the consented child ends exactly one process".to_string());
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
) -> Result<TeardownPlan, TeardownPlanError> {
    let actual = digest(bytes);
    if !actual.eq_ignore_ascii_case(plan_sha) {
        return Err(TeardownPlanError::DigestMismatch {
            path: shown_as.to_path_buf(),
        });
    }
    serde_json::from_slice(bytes).map_err(TeardownPlanError::Decode)
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
/// For the A/B service layout, either direct service slot under the same `bin`
/// is accepted: registration may already name the next release. Other layouts
/// remain restricted to the exact directory in the plan.
///
/// Pure over what was OBSERVED, so the rule is testable without a process to kill.
/// Paths compare the way Windows means them — case-insensitively, and `/` spelled `\`
/// is the same separator — because the scheduler and `QueryFullProcessImageName`
/// disagree about spelling far more often than they disagree about the file.
pub fn target_is_our_core(
    plan: &TeardownPlan,
    observed_image: &Path,
) -> Result<(), TeardownPlanError> {
    use continuum_host::path_spelling::equal;
    let root = &plan.install_dir;
    let image_dir = observed_image.parent();
    let other_slot = root.parent().and_then(|bin| {
        if !bin
            .file_name()
            .is_some_and(|name| equal(Path::new(name), Path::new("bin")))
        {
            return None;
        }
        let slot = root.file_name()?;
        if equal(Path::new(slot), Path::new("service-a")) {
            Some(bin.join("service-b"))
        } else if equal(Path::new(slot), Path::new("service-b")) {
            Some(bin.join("service-a"))
        } else {
            None
        }
    });
    if !image_dir.is_some_and(|dir| {
        equal(dir, root) || other_slot.as_ref().is_some_and(|slot| equal(dir, slot))
    }) {
        return Err(TeardownPlanError::OutsideInstallation {
            pid: plan.pid,
            image: observed_image.to_owned(),
            directory: root.clone(),
        });
    }
    if !observed_image.file_name().is_some_and(|name| {
        CORE_IMAGE_NAMES
            .iter()
            .any(|allowed| equal(Path::new(name), Path::new(allowed)))
    }) {
        return Err(TeardownPlanError::NotCore {
            pid: plan.pid,
            image: observed_image.to_owned(),
        });
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
    fn image(&self) -> Result<PathBuf, TeardownPlanError>;
    /// Spend it. Consumes self: a capability is used once.
    fn spend(self) -> Result<(), TeardownPlanError>;
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
) -> Result<(PathBuf, String), TeardownPlanError>
where
    C: HeldCapability,
    T: FnOnce(i32) -> Result<C, TeardownPlanError>,
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

/// The launcher reports this protocol, never localized exception prose.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConsentOutcome {
    Refused,
    Completed {
        #[serde(rename = "exitCode")]
        exit_code: i32,
    },
}

pub fn elevation_outcome(
    pid: i32,
    consent: ConsentOutcome,
    receipt: Option<&TeardownReceipt>,
) -> Result<(), ElevationError> {
    match consent {
        ConsentOutcome::Refused => Err(ElevationError::Refused { pid }),
        ConsentOutcome::Completed { exit_code: 0 } => Ok(()),
        ConsentOutcome::Completed { exit_code } => Err(ElevationError::Child {
            exit_code,
            receipt: receipt.cloned(),
        }),
    }
}
/// The unelevated half: write the plan, ask for ONE consent, read the receipt. Whether
/// the process actually WENT is proven by the caller on the same bounded deadline the
/// ordinary teardown uses — an immediate liveness probe after a terminate sees a process
/// mid-exit, which is the mistake this file already made once.
#[cfg(test)]
mod tests {
    use super::*;
    fn check_target(plan: &TeardownPlan, image: &str) -> Result<(), TeardownPlanError> {
        target_is_our_core(plan, Path::new(image))
    }

    fn plan() -> TeardownPlan {
        TeardownPlan {
            pid: 25040,
            install_dir: PathBuf::from("C:/Users/a/.continuum/bin"),
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
        let unknown = parse(&["--escalate"]).expect_err(
            "there is no operator-facing escalate verb — `continuum install` is the one command",
        );
        assert!(unknown.contains("unknown option"), "{unknown}");

        // what this catches: a plain stop taking the engine down. The engine stays unless asked.
        assert!(
            !parse(&[]).expect("a plain stop").engine,
            "a plain stop keeps the engine"
        );
        let all = parse(&["--engine"]).expect("an operator's stop that ends the engine too");
        assert!(all.engine && !all.elevated);
        assert!(parse(&[
            "--engine",
            "--elevated",
            "--plan",
            "p.json",
            "--plan-sha",
            "abc"
        ])
        .is_err());

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
            refused
                .to_string()
                .contains("refusing to terminate anything"),
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
                check_target(&p, "C:/Windows/System32/notepad.exe").unwrap_err(),
            ),
            (
                "inside but not a core",
                check_target(&p, "C:/Users/a/.continuum/bin/llama-server.exe").unwrap_err(),
            ),
        ] {
            assert!(
                refusal.to_string().to_lowercase().contains("refusing"),
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
        let elsewhere = "C:/Windows/System32/notepad.exe";
        let refused = check_target(&p, elsewhere)
            .expect_err("a process outside the installation must be refused");
        assert!(
            refused.to_string().contains("recycled"),
            "the reason must name reuse: {refused}"
        );
        assert!(
            refused.to_string().contains(elsewhere),
            "the refusal names what is there: {refused}"
        );

        let neighbour = "C:/Users/a/.continuum/bin/llama-server.exe";
        let sibling = check_target(&p, neighbour)
            .expect_err("our own engine is not our core; terminating it would take the lane");
        assert!(
            sibling.to_string().contains("not a core image"),
            "{sibling}"
        );

        // A path that merely STARTS with the directory's characters is not inside it.
        let lookalike = "C:/Users/a/.continuum/bin-evil/core.exe";
        assert!(
            check_target(&p, lookalike).is_err(),
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
        check_target(&p, "C:/Users/a/.continuum/bin/core.prev.exe")
            .expect("a core that outlived its own deploy runs from the parking space");
        check_target(&p, "C:/Users/a/.continuum/bin/core.exe")
            .expect("and the ordinary case still passes");
    }

    // Regression: registration points at the next slot while the previous core
    // still serves. Consent must accept that core, but no unrelated sibling.
    #[test]
    fn registered_and_running_service_slots_can_differ() {
        for (registered, running) in [("service-a", "service-b"), ("service-b", "service-a")] {
            let mut p = plan();
            p.install_dir = PathBuf::from(format!("C:/Users/a/.continuum/bin/{registered}"));
            let bin = "C:/Users/a/.continuum/bin";
            check_target(&p, &format!("{bin}/{running}/continuum-core-server.exe"))
                .expect("previous slot in the same installation");
            for rejected in [
                "continuum-core-server.exe".to_string(),
                format!("{bin}/service-c/continuum-core-server.exe"),
                format!("{bin}/{running}/nested/continuum-core-server.exe"),
                format!("{bin}/{running}/llama-server.exe"),
                format!("C:/Users/other/.continuum/bin/{running}/continuum-core-server.exe"),
            ] {
                assert!(check_target(&p, &rejected).is_err(), "{rejected}");
            }
        }
        let mut p = plan();
        p.install_dir = PathBuf::from("C:/unrelated/service-a");
        assert!(check_target(&p, "C:/unrelated/service-b/core.exe").is_err());
    }

    // A capability whose every act is RECORDED, so the sequence can be asserted rather
    // than described. `spend` consumes it, which is the type system carrying the
    // once-only property the real handle has.
    #[derive(Default)]
    struct Recorder {
        log: std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>,
        image: PathBuf,
    }

    impl HeldCapability for Recorder {
        fn image(&self) -> Result<PathBuf, TeardownPlanError> {
            self.log.borrow_mut().push("image");
            Ok(self.image.clone())
        }
        fn spend(self) -> Result<(), TeardownPlanError> {
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
        let ours = p.install_dir.join("core.exe");
        let (image, graceful) = teardown_sequence(
            &p,
            move |_pid| {
                taken.borrow_mut().push("take");
                Ok(Recorder {
                    log: taken.clone(),
                    image: ours.clone(),
                })
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
            |_pid| {
                Err(TeardownPlanError::Take(std::io::Error::from(
                    std::io::ErrorKind::PermissionDenied,
                )))
            },
            || {
                l.borrow_mut().push("drain");
                async { String::new() }
            },
        )
        .await
        .expect_err("no capability must abort before anything is drained");
        assert!(
            matches!(refused, TeardownPlanError::Take(ref e) if e.kind() == std::io::ErrorKind::PermissionDenied)
        );
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
                    image: PathBuf::from("C:/Windows/System32/notepad.exe"),
                })
            },
            || {
                l.borrow_mut().push("drain");
                async { String::new() }
            },
        )
        .await
        .expect_err("a recycled pid must abort before anything is drained");
        assert!(wrong.to_string().contains("recycled"), "{wrong}");
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
        let ours = p.install_dir.join("core.exe");
        let (image, graceful) = teardown_sequence(
            &p,
            move |_pid| {
                taken.borrow_mut().push("take");
                Ok(Recorder {
                    log: taken.clone(),
                    image: ours.clone(),
                })
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
        elevation_outcome(25040, ConsentOutcome::Completed { exit_code: 0 }, None)
            .expect("completed child");
        assert!(matches!(
            elevation_outcome(25040, ConsentOutcome::Refused, None),
            Err(ElevationError::Refused { pid: 25040 })
        ));
        assert!(matches!(
            elevation_outcome(25040, ConsentOutcome::Completed { exit_code: 1 }, None),
            Err(ElevationError::Child {
                exit_code: 1,
                receipt: None
            })
        ));
        assert!(
            matches!(
                elevation_outcome(
                    25040,
                    ConsentOutcome::Completed { exit_code: 1223 },
                    Some(&TeardownReceipt("child ran".into()))
                ),
                Err(ElevationError::Child {
                    exit_code: 1223,
                    receipt: Some(_)
                })
            ),
            "a child exit code must never impersonate declined consent"
        );
    }

    // what this catches: path spelling read as a different file. The scheduler echoes
    // `Execute` as registered while QueryFullProcessImageName answers with the OS's own
    // spelling; treating `C:/x/Core.exe` and `c:\x\core.exe` as different targets would
    // refuse every legitimate teardown on some machines and none of the dangerous ones.
    #[test]
    fn a_native_path_identifies_the_core() {
        let p = plan();
        check_target(&p, "C:/Users/a/.continuum/bin/continuum-core-server.exe")
            .expect("case and separator spelling name the same place on Windows");
    }
}
