#![cfg(test)]
use super::*;

fn ptable(pairs: &[(i32, i32)]) -> std::collections::HashMap<i32, i32> {
    pairs.iter().copied().collect()
}

/// The socket a running core is bound to, and the refusal line that names it.
///
/// Measured 2026-09-04 on the Intel Mac: pid 83712 served
/// `~/.continuum/intelmac-core.sock` for an hour while every client-side diagnostic
/// reported `/tmp/continuum-core.sock` — the one path that was not the answer.
mod bound_socket {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    /// what this catches: re-deriving `main.rs`'s argument rule instead of calling it.
    /// BOTH spellings of the flag must be peeled — `boot_mode.rs` accepts
    /// `--mode VALUE` as well as `--mode=VALUE`, and the space form is the one the
    /// original heuristic got wrong: "first arg not starting with `-`" returns
    /// **"fail-fast"**, a boot mode reported to the operator as a socket path and
    /// pasted into their remedy line. The equals form alone passes either way, which
    /// is why the first version of this test looked right and proved nothing.
    #[test]
    fn both_flag_spellings_are_peeled_not_just_the_equals_form() {
        assert_eq!(
            socket_from_core_argv(&argv(&[
                "continuum-core-server",
                "--mode",
                "fail-fast",
                "/tmp/a.sock",
            ])),
            Some("/tmp/a.sock".to_string()),
            "the SPACE form's value must never be reported as a socket path"
        );
        assert_eq!(
            socket_from_core_argv(&argv(&[
                "continuum-core-server",
                "--mode=full-citizen",
                "/tmp/a.sock",
            ])),
            Some("/tmp/a.sock".to_string()),
            "a flag before the positional must not be mistaken for the socket"
        );
        assert_eq!(
            socket_from_core_argv(&argv(&["continuum-core-server", "/tmp/a.sock"])),
            Some("/tmp/a.sock".to_string())
        );
    }

    /// what this catches: inventing a socket for a core that passed none. Such a core
    /// resolved the path from ITS environment at launch, which this process cannot read
    /// afterwards — so the honest answer is "unknown", and a `Some(default)` here would
    /// make the hint assert a path the core may well not be on. Both flag spellings,
    /// because the space form is where a "skip the dashes" reading leaks the VALUE.
    #[test]
    fn a_core_with_no_positional_reports_unknown_rather_than_the_default() {
        assert_eq!(
            socket_from_core_argv(&argv(&["continuum-core-server", "--mode=fail-fast"])),
            None
        );
        assert_eq!(
            socket_from_core_argv(&argv(&["continuum-core-server", "--mode", "fail-fast"])),
            None,
            "the space form's value is a boot mode, not an unknown socket"
        );
    }

    /// what this catches: reporting a socket for a core that never bound one. A
    /// malformed `--mode` makes `main.rs` print its error and `exit(2)` BEFORE any
    /// socket is resolved, so such a process provably bound nothing — "unknown" is
    /// the literally correct answer, not a swallowed error. The pid is never hidden
    /// by this: the refusal lists it from `running_core_pids` either way. Only the
    /// unprovable socket claim is withheld, which is this helper's whole contract.
    #[test]
    fn a_malformed_mode_reports_unknown_because_that_core_bound_nothing() {
        assert_eq!(
            socket_from_core_argv(&argv(&[
                "continuum-core-server",
                "--mode",
                "not-a-real-mode",
                "/tmp/a.sock",
            ])),
            None,
            "a core that exits on a bad --mode never reached socket resolution"
        );
    }

    /// what this catches: the hint firing on the ordinary wedged core. A core bound to
    /// the very socket the CLI is using is genuinely not answering; telling that
    /// operator to "reach it elsewhere" would send them chasing a door that is already
    /// open, so the existing message must go out unchanged.
    #[test]
    fn no_hint_when_the_running_core_is_bound_to_the_socket_we_asked_for() {
        let bound = vec![(1, "/tmp/continuum-core.sock".to_string())];
        assert_eq!(
            bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock"),
            None
        );
        assert_eq!(bound_elsewhere_hint(&[], "/tmp/continuum-core.sock"), None);
    }

    /// what this catches: a hint that reports the mismatch without the remedy. The
    /// operator's next keystroke needs the core's ACTUAL path — naming only the
    /// client's is what cost four commands and a wrong "wedged" hypothesis.
    #[test]
    fn hint_names_the_cores_path_and_the_command_that_reaches_it() {
        let bound = vec![(83712, "/home/agent/.continuum/node-core.sock".to_string())];
        let hint = bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock")
            .expect("a core bound elsewhere must produce a hint");
        assert!(
            hint.contains("pid 83712 → /home/agent/.continuum/node-core.sock"),
            "must name which pid is where: {hint}"
        );
        assert!(
            hint.contains("CONTINUUM_CORE_SOCKET=/home/agent/.continuum/node-core.sock"),
            "the remedy must carry the core's path, not the client's: {hint}"
        );
        assert!(
            hint.contains("NOT wedged"),
            "with nothing on our socket, the hint's job is to rule out the wedged case: {hint}"
        );
    }

    /// what this catches: a mixed fleet hiding the outlier — AND the headline denying
    /// the other half of it. With one core on the client's socket and one elsewhere,
    /// only the unreachable core belongs in the list; but the core on our socket IS
    /// genuinely wedged, so leading with an unconditional "NOT wedged" would tell the
    /// operator their real problem does not exist. The earlier version of this test
    /// pinned the right list around exactly that false claim by never asserting on the
    /// headline at all.
    #[test]
    fn mixed_fleet_lists_only_the_outlier_and_does_not_deny_the_wedged_core() {
        let bound = vec![
            (7, "/tmp/continuum-core.sock".to_string()),
            (9, "/home/agent/.continuum/node-core.sock".to_string()),
        ];
        let hint = bound_elsewhere_hint(&bound, "/tmp/continuum-core.sock").expect("hint");
        assert!(
            hint.contains("pid 9"),
            "the unreachable core must appear: {hint}"
        );
        assert!(
            !hint.contains("pid 7"),
            "a core on the asked-for socket is not 'elsewhere': {hint}"
        );
        assert!(
            !hint.contains("NOT wedged"),
            "pid 7 IS on our socket and unresponsive — the hint must not deny it: {hint}"
        );
    }
}

/// The deploy watchdog: is a slow build failing, or just slow?
mod launch_wait {
    use super::*;

    /// what this catches: a WALL-CLOCK bound standing in for a liveness check.
    ///
    /// Both of these are REAL measurements from 2026-09-05 and both deploys SUCCEEDED —
    /// 1,404s for a full CUDA rebuild on the 5090, and an Intel Mac cold build that ran
    /// past the 1,800s ceiling and brought the core up about forty minutes after the CLI
    /// had already reported failure. A verdict keyed on elapsed time calls both of those
    /// dead. Keyed on silence, both are alive, which is the only answer that is correct
    /// on both machines.
    #[test]
    fn a_long_build_that_is_still_talking_is_not_a_failure() {
        let stall = 5 * 60;
        // 23 minutes elapsed means nothing; 10s since the last log line means everything.
        assert_eq!(wait_verdict(10, stall), WaitVerdict::Progressing);
        // Even at the far end of a 40-minute build, recent output is life.
        assert_eq!(wait_verdict(stall - 1, stall), WaitVerdict::Progressing);
    }

    /// what this catches: regression for card 6c91036c — the watchdog killing a HEALTHY
    /// build and reporting it "burned no CPU" while rustc sat at 631% in its own process
    /// group. Reproduced on a real deploy of f49409fa2 on the Intel Mac.
    ///
    /// The readings here are the MEASURED ones, taken 20s apart mid-build: the group total
    /// was ~2589s while a 43-minute rustc was alive, then ~3s once that child finished its
    /// crate and cargo replaced it. Group cputime sums only members alive at that instant,
    /// so a child exiting REMOVES its accumulated time and the total drops.
    ///
    /// THIS TEST MUST DRIVE THE READING DOWN. A test that only ever feeds increasing values
    /// passes against `reading > last` — the buggy comparison — which is presumably how the
    /// defect shipped with the rest of the watchdog well covered. The descent IS the bug.
    #[test]
    fn a_cpu_reading_that_drops_is_progress_because_a_finished_child_leaves_the_group() {
        // The measured pair. Under `>` this returns false and the stall clock never resets
        // again for the life of the build.
        assert!(
            cpu_reading_shows_progress(3, 2589),
            "a group total that FELL because a long rustc finished its crate is a build \
                 making progress, not a dead one"
        );
        // Ordinary within-crate accumulation is still progress.
        assert!(cpu_reading_shows_progress(2589, 3));
        // And a build that is genuinely stuck holds the sum CONSTANT — the one reading that
        // must NOT reset the clock, or the watchdog never fires at all.
        assert!(
            !cpu_reading_shows_progress(2589, 2589),
            "an unchanged group total is the signature of a hung exec: it burns no CPU and \
                 spawns no children, and it is the only thing this watchdog exists to catch"
        );
        // A climbing total reads as progress, which is what keeps an ordinary build alive.
        // It also means a SPINNING hang is not caught by this watchdog at all: the CPU arm
        // and the log arm reset the same clock, so a spin resets it forever and only
        // MAX_WAIT_SECS bounds it. True of `>` as well — not a regression, just not a
        // guarantee. Asserted so nobody 'fixes' this into an equality check believing the
        // log arm would still cover the spin; it would not.
        assert!(cpu_reading_shows_progress(2590, 2589));
    }

    /// what this catches: the opposite failure — never giving up. A script that has said
    /// nothing for the full stall window has stopped, and the operator needs to be told
    /// that rather than left watching a spinner. The boundary is inclusive so the limit
    /// means "this long is too long", not "this long is fine".
    #[test]
    fn silence_past_the_limit_is_a_stall_and_the_boundary_is_inclusive() {
        let stall = 5 * 60;
        assert_eq!(wait_verdict(stall, stall), WaitVerdict::Stalled);
        assert_eq!(wait_verdict(stall + 1, stall), WaitVerdict::Stalled);
    }

    /// what this catches: misreading `ps -o cputime=`, which is the second progress signal
    /// and the fix for a regression I shipped — a stall clock reset ONLY by log lines
    /// condemns cargo compiling one large crate quietly, which is exactly the cold low-end
    /// build the watchdog protects. Every shape `ps` emits must parse, because a field this
    /// function silently reads as 0 turns a working build back into a false stall.
    #[test]
    fn every_ps_cputime_shape_parses() {
        assert_eq!(cputime_to_secs("0:05"), 5, "mm:ss");
        assert_eq!(cputime_to_secs("12:34"), 12 * 60 + 34);
        assert_eq!(cputime_to_secs("1:02:03"), 3600 + 2 * 60 + 3, "hh:mm:ss");
        assert_eq!(
            cputime_to_secs("2-03:04:05"),
            2 * 86_400 + 3 * 3600 + 4 * 60 + 5,
            "dd-hh:mm:ss — the day field arrives glued to the hour"
        );
        assert_eq!(
            cputime_to_secs("   7:08  "),
            7 * 60 + 8,
            "ps pads its column"
        );
    }

    /// what this catches: a garbage field silently reading as a huge or negative number and
    /// either freezing the stall clock forever or resetting it every tick. Unparseable means
    /// ZERO contribution — the caller then falls back to the log signal, which is the
    /// refuse-to-guess posture the rest of tonight's fixes landed on.
    #[test]
    fn unparseable_cputime_contributes_nothing_rather_than_guessing() {
        assert_eq!(cputime_to_secs(""), 0);
        assert_eq!(cputime_to_secs("   "), 0);
        assert_eq!(cputime_to_secs("?"), 0);
        assert_eq!(cputime_to_secs("not:a:time"), 0);
    }
}

mod launch_policy {
    use super::*;

    /// what this catches: `reboot` — THE deploy path — silently running a
    /// prebuilt artifact instead of the source just edited. Regression for
    /// 7e0c5469a, which made the installed binary the default for BOTH
    /// callers of `launch_core`; the reboot banner still promised "building
    /// fresh binary, then swapping" while exec'ing a month-old binary, so no
    /// edit could reach the running core at all.
    #[test]
    fn reboot_builds_from_source_even_when_an_installed_binary_exists() {
        assert_eq!(
            plan_launch(LaunchSource::FromSource, false, true, true),
            LaunchPlan::Script
        );
    }

    /// what this catches: re-breaking `start` for the no-source-tree user
    /// (the case 7e0c5469a was written for) while fixing reboot. `start`
    /// wants a RUNNING core, not a fresh one — it must never compile when an
    /// artifact is sitting right there.
    #[test]
    fn start_prefers_the_installed_binary_over_a_compile() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, false, true, true),
            LaunchPlan::Installed
        );
    }

    /// what this catches: a reboot on an installed node (no checkout) either
    /// dying with "no start script" or — worse — quietly calling itself a
    /// deploy. It restarts the artifact, and the distinct variant is what
    /// forces the caller to SAY nothing was rebuilt.
    #[test]
    fn reboot_without_a_source_tree_restarts_the_artifact_and_says_so() {
        assert_eq!(
            plan_launch(LaunchSource::FromSource, false, false, true),
            LaunchPlan::InstalledWithoutRebuild
        );
    }

    /// what this catches: substituting a prebuilt binary for an explicit
    /// operator demand to compile. CONTINUUM_FROM_SOURCE asks a specific
    /// question; answering a different one silently is the fallback class
    /// this codebase forbids.
    #[test]
    fn an_explicit_from_source_request_fails_loud_with_no_script() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, true, false, true),
            LaunchPlan::NoLaunchable
        );
    }

    /// what this catches: the env override being ignored on the `start` path
    /// once the policy argument existed — two ways to ask for a source build
    /// and only one honoured.
    #[test]
    fn the_env_override_still_forces_a_source_build_on_start() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, true, true, true),
            LaunchPlan::Script
        );
    }

    /// what this catches: a bare machine (no artifact, no checkout) getting a
    /// launch attempt against nothing instead of one clear error.
    #[test]
    fn nothing_installed_and_no_script_is_a_loud_nothing_to_launch() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, false, false, false),
            LaunchPlan::NoLaunchable
        );
        assert_eq!(
            plan_launch(LaunchSource::FromSource, false, false, false),
            LaunchPlan::NoLaunchable
        );
    }

    /// what this catches: `start` in a fresh checkout before anything is
    /// installed — the fresh-clone front door (#291). Compiling is correct
    /// here; refusing is not.
    #[test]
    fn start_in_a_checkout_with_no_installed_binary_builds() {
        assert_eq!(
            plan_launch(LaunchSource::Installed, false, true, false),
            LaunchPlan::Script
        );
    }
}

/// what this catches: the orphan classifier deciding to KILL a live serving
/// lane. `reboot` reaps every owned engine process not descended from a live
/// core, so a wrong answer here terminates a 21 GB in-service llama-server
/// mid-request. Grandchildren count as descended — engines are spawned
/// through intermediate processes, so a depth-1 check would flag them all.
#[test]
fn in_service_descendants_are_never_orphans() {
    // core 100 -> shim 200 -> engine 300
    let parents = ptable(&[(300, 200), (200, 100)]);
    assert!(
        descends_from(&parents, 300, &[100]),
        "grandchild is in service"
    );
    assert!(descends_from(&parents, 200, &[100]));
    assert!(descends_from(&parents, 100, &[100]), "the core itself");
}

/// what this catches: card d746a1a0 — Windows `/T` on the core killed the
/// recorded gateway before reboot's later orphan exclusion could protect it.
/// Inspect the commands production executes, including the orphan-sweep pass.
#[test]
fn windows_teardown_preserves_only_the_verified_lane_subtree() {
    let parents = ptable(&[
        (100, 1),     // core
        (200, 100),   // gateway launcher, itself expendable
        (8600, 200),  // identity-verified live lane
        (8610, 8600), // helper owned by that lane
        (300, 100),   // eye-node
        (301, 300),   // browser
        (400, 100),   // other worker
        (500, 100),   // stale/unrecorded serving process: not protected
        (501, 500),
        (900, 1), // unrelated process: never a teardown root
    ]);
    let command_args = |plan: Vec<KillStep>| {
        let mut args: Vec<Vec<String>> = plan
            .into_iter()
            .map(|kill| {
                let cmd = kill.command();
                assert_eq!(cmd.get_program(), "taskkill");
                cmd.get_args()
                    .map(|arg| arg.to_str().unwrap().to_owned())
                    .collect()
            })
            .collect();
        args.sort();
        args
    };
    // Duplicate/descendant roots must not produce overlapping tree kills.
    let roots = [300, 200, 100, 100, 301, 500];
    let plan = kill_plan(&roots, &parents, &[8600]);
    assert_eq!(
        plan.first(),
        Some(&KillStep::Process(100)),
        "stop the spawning core first"
    );
    assert_eq!(
        command_args(plan),
        vec![
            vec!["/F", "/PID", "100"],
            vec!["/F", "/PID", "200"],
            vec!["/F", "/T", "/PID", "300"],
            vec!["/F", "/T", "/PID", "400"],
            vec!["/F", "/T", "/PID", "500"],
        ]
    );
    // A surviving launcher ancestor in the owned-orphan pass must ALSO be
    // killed alone, never with /T across the now-adoptable lane beneath it.
    assert_eq!(
        command_args(kill_plan(
            &[200, 300, 301, 400, 500, 501, 8600, 8610],
            &parents,
            &[8600],
        )),
        vec![
            vec!["/F", "/PID", "200"],
            vec!["/F", "/T", "/PID", "300"],
            vec!["/F", "/T", "/PID", "400"],
            vec!["/F", "/T", "/PID", "500"],
        ]
    );
    // Full stop — or no identity-verified lane record — still kills the
    // entire core tree, including the otherwise-preserved gateway.
    assert_eq!(
        command_args(kill_plan(&roots, &parents, &[])),
        vec![vec!["/F", "/T", "/PID", "100"]]
    );
    // An old fixed 64-hop ancestry limit must not turn a deeper protected
    // branch into an apparently unprotected /T target.
    let deep: std::collections::HashMap<i32, i32> =
        (1..=80).map(|pid| (pid, pid - 1)).collect();
    let plan = kill_plan(&[1], &deep, &[80]);
    assert_eq!(plan.len(), 79);
    assert!(plan
        .iter()
        .all(|kill| matches!(kill, KillStep::Process(_))));
}

// what this catches (IntelMac 2026-09-21 11:20Z, card 3ef0986c): the deploy consumer
// is the CORE'S CHILD, so the tree it reaps on a reboot contains itself. With the
// consumer in the keep set (what `keep_with_self` adds at the call site), the core
// gets a `Process` step — it dies alone — and no step names the consumer or its
// branch; without it, `Tree(core)` reaps the hand mid-handoff and the kickstart the
// node is owed is never issued. Both directions: the lane keep still holds beside it.
#[test]
fn the_reaper_is_never_in_its_own_plan() {
    let (core, consumer, lane, eye) = (100, 200, 300, 400);
    let parents = std::collections::HashMap::from([
        (consumer, core),
        (lane, core),
        (eye, core),
        (core, 1),
    ]);
    let plan = kill_plan(&[core], &parents, &keep_with_self_for_test(&[lane], consumer));
    assert_eq!(plan.first(), Some(&KillStep::Process(core)), "the core dies alone, never as a tree");
    assert!(
        !plan.iter().any(|s| matches!(s, KillStep::Tree(p) | KillStep::Process(p) if *p == consumer)),
        "the reaper never appears in its own plan: {plan:?}"
    );
    assert!(!plan.iter().any(|s| matches!(s, KillStep::Tree(p) | KillStep::Process(p) if *p == lane)), "the lane keep still holds");
    assert!(plan.contains(&KillStep::Tree(eye)), "an unprotected sibling is still reaped: {plan:?}");
    // The pre-fix shape, for contrast: with only the lane kept, the consumer IS in the plan.
    let unprotected = kill_plan(&[core], &parents, &[lane]);
    assert!(unprotected.contains(&KillStep::Tree(consumer)), "without the self keep the hand is reaped: {unprotected:?}");
}

/// `keep_with_self` pins the CALLER's pid, which in a test is the test binary; this
/// mirrors it with an explicit self so the plan can be asserted for a chosen pid.
fn keep_with_self_for_test(keep: &[i32], self_pid: i32) -> Vec<i32> {
    let mut with_self = keep.to_vec();
    with_self.push(self_pid);
    with_self
}

/// what this catches: the actual BIGMAMA leak. llama-server pid 37148 ran
/// 24h past its dead parent, holding 127.0.0.1:8090, so every persona that
/// resolved a chat model through that port got an EMBEDDING model. Its
/// parent is absent from the table entirely (dead), which must read as
/// "orphan", not as "unknown, leave it alone".
/// what this catches (card 59052747, 2026-09-20): the Unix executor of the kill plan
/// — until now `let _ = keep;` and a group kill per root, so the plan's `Process(core)`
/// never existed on macOS/Linux and the live lane died with the core on every reboot.
/// Real processes, one group: a leader and its child; a plan that protects the child
/// must leave it alive after the leader is gone, and the same plan with nothing
/// protected must take the whole group (the plain `stop`).
#[cfg(unix)]
#[test]
fn a_protected_child_survives_its_leaders_kill_on_unix() {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    fn alive(pid: i32) -> bool {
        unsafe { libc::kill(pid, 0) == 0 }
    }
    fn group(protect: bool) -> (i32, i32) {
        // A leader that execs nothing but waits on one child in ITS group.
        let mut leader = Command::new("sh")
            .args(["-c", "sleep 30 & wait"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .expect("sh spawns"); // test: the shell exists on every unix CI box
        let leader_pid = leader.id() as i32;
        // Find the child (the sleep) under the leader.
        let child = (0..50)
            .find_map(|_| {
                std::thread::sleep(std::time::Duration::from_millis(50));
                let mut sys = System::new();
                sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
                process_parents(&sys)
                    .into_iter()
                    .find(|(_, parent)| *parent == leader_pid)
                    .map(|(child, _)| child)
            })
            .expect("the leader's child appears"); // test: `sleep` forks within 2.5 s
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        let parents = process_parents(&sys);
        let keep: Vec<i32> = if protect { vec![child] } else { vec![] };
        let plan = kill_plan(&[leader_pid], &parents, &keep);
        assert_eq!(plan, vec![if protect { KillStep::Process(leader_pid) } else { KillStep::Tree(leader_pid) }]);
        for step in plan {
            step.execute();
        }
        let _ = leader.wait();
        std::thread::sleep(std::time::Duration::from_millis(200));
        (leader_pid, child)
    }
    let (leader, child) = group(true);
    assert!(!alive(leader), "the leader is gone");
    assert!(alive(child), "the protected child in the leader's group survives its kill");
    unsafe { libc::kill(child, libc::SIGKILL) };
    let (leader, child) = group(false);
    assert!(!alive(leader));
    assert!(!alive(child), "nothing protected: the group goes with the leader (plain stop)");
}

#[test]
fn a_process_whose_parent_is_gone_is_an_orphan() {
    let parents = ptable(&[(37148, 37856)]); // 37856 itself not present = dead
    assert!(!descends_from(&parents, 37148, &[37920]));
    // And with no core running at all, nothing is in service.
    assert!(!descends_from(&parents, 37148, &[]));
}

/// what this catches: a HANG in `continuum reboot`. A pid table can present
/// a cycle — pid reuse during a racing scan, or a parent pointer into a
/// descendant — and an unbounded ancestor walk would spin forever inside the
/// verb whose entire job is recovering from unknown failures.
#[test]
fn a_cyclic_parent_chain_terminates() {
    let parents = ptable(&[(1, 2), (2, 3), (3, 1)]);
    assert!(
        !descends_from(&parents, 1, &[999]),
        "must terminate, not hang"
    );
    // A cycle that CONTAINS a kept pid still resolves as in-service.
    assert!(descends_from(&parents, 1, &[3]));
}

/// what this catches: pid 0 / self-parent as chain terminators. Both
/// platforms present them, and treating either as a real hop would walk a
/// bogus ancestor or loop.
#[test]
fn root_sentinels_terminate_the_chain() {
    assert!(!descends_from(&ptable(&[(5, 0)]), 5, &[7]));
    assert!(!descends_from(&ptable(&[(5, 5)]), 5, &[7]));
}
