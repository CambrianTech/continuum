//! deploy_provenance.rs — the pure "is what is RUNNING what we shipped?" decisions.
//!
//! ## Why these live in the lib and not the `continuum` bin
//!
//! They were born in `bin/continuum.rs`, correctly written as pure string-in/string-out
//! functions, and correctly unit-tested — but CI runs `cargo test -p continuum-core --lib`,
//! which does not compile bin test mods. The #194 guard (the check that exists because a
//! reboot once shipped a stale binary and reported success) was therefore protected by
//! tests **nothing ran**. Same trap as [`super::core_bind_guard`]: a correct check whose
//! coverage is unreachable is a check you will break without noticing.
//!
//! The bin keeps everything impure — the ping, `git rev-parse`, artifact resolution,
//! process description — and calls into here for the verdicts.
//!
//! ## The two questions, deliberately separate
//!
//! 1. [`deploy_verdict`] — did the CORE swap take? Authoritative; every gap is an ERROR.
//! 2. [`cli_staleness_note`] — is the CLI that asked the question itself current? A
//!    WARNING, never fatal (see its docs for why the asymmetry is intentional).

/// The pure compare at the heart of deploy-verify: running core's self-reported SHA vs the
/// SHA the deploy shipped. `Ok(success line)` only on a REAL match; every gap is a loud
/// error naming both SHAs and both identities.
///
/// NEVER skips soft: a core that reports no `buildSha` (pre-#194, therefore stale),
/// `unknown` provenance, or an outright mismatch are all errors. A reboot must not print a
/// success line it cannot back with provenance ([[fallbacks-are-illegal-fail-loud]]).
pub fn deploy_verdict(
    actual: Option<&str>,
    expected: &str,
    expected_source: &str,
    running_desc: &str,
) -> Result<String, String> {
    let actual = match actual {
        Some(a) if !a.is_empty() => a,
        _ => {
            return Err(format!(
                "DEPLOY MISMATCH (#194): the running core ({running_desc}) does not report a build \
                 SHA on ping — it is a pre-#194 (or otherwise stale) binary, so the swap did NOT \
                 happen. Expected build {expected} ({expected_source}). Do not trust any live test: \
                 stop the old core and reboot again."
            ))
        }
    };
    if actual == "unknown" || expected == "unknown" {
        return Err(format!(
            "DEPLOY UNVERIFIABLE (#194): build provenance is 'unknown' (running core \
             ({running_desc}) reports {actual}; expected {expected} from {expected_source}) — a \
             binary was built outside a git tree, so freshness cannot be proven. Rebuild inside \
             the git checkout and reboot again; never trust an unverifiable deploy."
        ));
    }
    if sha_matches(actual, expected) {
        Ok(format!(
            "✅ deploy verified: core is running build {actual} (== {expected_source})"
        ))
    } else {
        Err(format!(
            "DEPLOY MISMATCH (#194): the running core ({running_desc}) is build {actual}, but the \
             deploy shipped build {expected} ({expected_source}). The swap did NOT take — a \
             stale binary is still serving while the reboot would have claimed success. Do not \
             trust any live test until this is fixed: rebuild cleanly (`cargo build -p \
             continuum-core --bin continuum-core-server`) and reboot again."
        ))
    }
}

/// Is the `continuum` CLI itself as fresh as the deploy? `None` when it matches (say
/// nothing, so the note means something when it appears), `Some(warning)` otherwise.
///
/// ## Why deploy-verify was half-blind without this
///
/// [`deploy_verdict`] answers exactly one question — did the CORE swap take — and answers
/// it well. But `continuum reboot` sets `CONTINUUM_SKIP_SELF_BUILD` and so never rebuilds
/// the CLI (deliberate: a running image cannot be replaced on Windows, and letting the
/// start script build it failed the whole cargo invocation and skipped the core build).
/// The consequence went unwritten: a fix living in the CLI cannot reach an operator through
/// the documented deploy path, and deploy-verify printed its green checkmark anyway because
/// it only ever looked at the core.
///
/// Measured 2026-08-14, same machine, minutes apart: the installed CLI's `stop` reaped only
/// the pidfile core and left pid 6453 alive and silent; the freshly-built CLI's `stop` then
/// printed SPLIT BRAIN and reaped it. #2287 had merged and had NOT reached the installed
/// CLI — while deploy-verify reported "✅ core is running build 1de9cebc5". That is #194 one
/// tier up: a deploy check that can only see half the system.
///
/// ## Why this WARNS instead of failing
///
/// The core verdict stays authoritative. A stale CLI is a different fact from a failed swap,
/// and making it an error would break deploy-verify for every operator whose CLI predates
/// their core — today, all of them. Loud and non-blocking kills the silent case without
/// turning a true green into a red. See #422 for fixing the rebuild itself.
pub fn cli_staleness_note(
    cli_sha: &str,
    expected: &str,
    expected_source: &str,
) -> Option<String> {
    // Unjudgeable provenance is reported, not swallowed — but briefly, because a binary
    // built outside a git tree is a legitimate state, unlike a core with no SHA at all.
    if cli_sha.is_empty() || cli_sha == "unknown" || expected.is_empty() || expected == "unknown" {
        return Some(format!(
            "⚠ CLI provenance unverifiable: this `continuum` reports build {}, deploy expected {} \
             ({expected_source}). Built outside a git tree, so CLI freshness cannot be proven \
             (#422).",
            if cli_sha.is_empty() { "none" } else { cli_sha },
            if expected.is_empty() { "none" } else { expected },
        ));
    }
    if sha_matches(cli_sha, expected) {
        return None;
    }
    Some(format!(
        "⚠ STALE CLI (#422): the core verdict above stands, but the `continuum` you just ran is \
         build {cli_sha}, not {expected} ({expected_source}). `reboot` rebuilds the CORE and never \
         the CLI, so any lifecycle fix that lives in the CLI — `start`/`stop`/`reboot`/\
         `deploy-verify` itself — is NOT deployed on this machine. Rebuild and reinstall the CLI \
         before trusting CLI-side behaviour."
    ))
}

/// Two short/long git SHAs refer to the same commit when one prefixes the other (git's
/// `--short` abbreviation length varies over a repo's life). Both must be real hex SHAs of
/// credible length — never matches `""` or `"unknown"`.
pub fn sha_matches(a: &str, b: &str) -> bool {
    let credible = |s: &str| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit());
    credible(a) && credible(b) && (a.starts_with(b) || b.starts_with(a))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the #194 regression — deploy-verify passing soft. Every gap (a
    // running core that reports no buildSha = pre-#194 = stale, 'unknown' provenance, an
    // outright mismatch) must be an ERROR naming both SHAs and both identities, and the
    // success line must appear ONLY on a real match. Turning any of these back into a
    // warning + Ok re-creates "core ready" as a false deploy receipt.
    #[test]
    fn deploy_verdict_never_passes_soft() {
        let running = "socket=/tmp/x.sock, pid(s) 42, image /t/debug/continuum-core-server";

        let ok = deploy_verdict(
            Some("abc123f"),
            "abc123f",
            "git HEAD of this checkout",
            running,
        )
        .expect("matching SHAs verify");
        assert!(ok.contains("✅ deploy verified"), "got {ok}");
        assert!(ok.contains("abc123f"), "names the build: {ok}");
        assert!(
            deploy_verdict(Some("abc123f00d"), "abc123f", "src", running).is_ok(),
            "prefix-tolerant across git --short abbreviation drift"
        );

        let err = deploy_verdict(
            Some("dead111"),
            "beef222",
            "artifact /usr/local/bin/x",
            running,
        )
        .expect_err("mismatch must fail");
        for needle in ["dead111", "beef222", running, "/usr/local/bin/x", "MISMATCH"] {
            assert!(err.contains(needle), "error names {needle}: {err}");
        }

        assert!(
            deploy_verdict(None, "beef222", "src", running).is_err(),
            "a core with no buildSha is stale, never a pass"
        );
        assert!(deploy_verdict(Some("unknown"), "beef222", "src", running).is_err());
        assert!(deploy_verdict(Some("dead111"), "unknown", "src", running).is_err());
    }

    // what this catches: deploy-verify going half-blind again. It answers "did the CORE
    // swap take" and printed a green checkmark while the CLI that ran it was provably older
    // (measured 2026-08-14: the installed CLI's `stop` left a survivor the fresh one reaped
    // with SPLIT BRAIN — #2287 merged but not deployed). Silence on a stale CLI is the
    // defect; a matching CLI must stay silent so the note means something when it fires.
    #[test]
    fn a_stale_cli_is_reported_and_a_fresh_one_is_silent() {
        assert_eq!(
            cli_staleness_note("abc123f", "abc123f", "git HEAD of this checkout"),
            None,
            "a CLI that matches the deploy says nothing"
        );
        assert_eq!(
            cli_staleness_note("abc123f00d", "abc123f", "src"),
            None,
            "prefix-tolerant, same as the core verdict — abbreviation drift is not staleness"
        );

        let note = cli_staleness_note("dead111", "beef222", "git HEAD of this checkout")
            .expect("a diverged CLI must be reported");
        for needle in ["dead111", "beef222", "#422", "STALE CLI"] {
            assert!(note.contains(needle), "note names {needle}: {note}");
        }
    }

    // what this catches: swallowing the unjudgeable case. A CLI built outside a git tree
    // reports "unknown"; silence there would be a soft skip of exactly the check this
    // exists to be — and it must still never assert staleness it cannot prove.
    #[test]
    fn unverifiable_cli_provenance_is_stated_not_swallowed() {
        for (cli, expected) in [("unknown", "beef222"), ("dead111", "unknown"), ("", "beef222")] {
            let note = cli_staleness_note(cli, expected, "src")
                .unwrap_or_else(|| panic!("cli={cli} expected={expected} must report"));
            assert!(note.contains("unverifiable"), "got {note}");
            assert!(
                !note.contains("STALE CLI"),
                "never assert staleness it cannot prove: {note}"
            );
        }
    }

    // what this catches: a loosened SHA compare quietly making every verdict vacuous —
    // empty/"unknown"/short/non-hex must never count as a match in EITHER direction.
    #[test]
    fn sha_matches_only_on_credible_hex_prefixes() {
        assert!(sha_matches("abc123f", "abc123f"));
        assert!(sha_matches("abc123f00d", "abc123f"));
        assert!(sha_matches("abc123f", "abc123f00d"));
        for (a, b) in [
            ("", ""),
            ("unknown", "unknown"),
            ("abc123f", ""),
            ("abc123", "abc123"),       // too short to be credible
            ("zzzzzzz", "zzzzzzz"),     // not hex
            ("abc123f", "def456a"),
        ] {
            assert!(!sha_matches(a, b), "{a:?} vs {b:?} must not match");
        }
    }
}
