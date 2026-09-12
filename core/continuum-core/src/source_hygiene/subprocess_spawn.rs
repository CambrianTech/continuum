//! **A subprocess is a liability the caller inherits. The count only goes down, and the
//! programs a library already answers for never go up.**
//!
//! 2026-09-12 01:0xZ on the M5: the core's descriptor table filled (a stream re-open
//! storm; see `persona/command_inbound_pump.rs`) and every `Command::new` on the node
//! failed with EBADF at once — the serving lane's `--list-devices` probe, `web/fetch`'s
//! browser, the lane relaunch. Each site reported its own symptom; none said "spawn".
//! Joel, the same hour: *"some AIs forced in random exec and shell stuff where managed
//! service modules belonged, with library calls to airc for example."* Measured that
//! night: 166 spawn sites in this crate, only four files on the managed
//! [`bounded_command`](crate::system_resources::bounded_command); six spawn the `airc`
//! CLI from a process that links airc-lib, seven fork `ps` beside a `sysinfo`
//! dependency, ten run `tar`/`cp` where `std::fs` does the job.
//!
//! # The rule
//!
//! 1. The number of `Command::new(` sites in production code may never rise
//!    ([`BASELINE_SPAWN_SITES`]).
//! 2. For the programs in [`ANSWERED_BY_A_LIBRARY`] — a binary this process already
//!    links, or a question `std`/`sysinfo` answers — the per-program count may never
//!    rise, and each carries the replacement that retires it. New code reaches for the
//!    library; a site that must remain says why on its line.
//!
//! A real subprocess (llama-server, git, cargo, a benchmark's own test runner, the
//! browser) is fine — through `bounded_command`, with a bound and a named outcome.
use super::{split_code_and_comment, SourceFile, SourceRule, Violation};

/// Programs that a library, module, or std already answers for. The replacement is
/// the rule's message, so the site that trips it also says what to write instead.
pub const ANSWERED_BY_A_LIBRARY: &[(&str, &str)] = &[
    ("airc", "airc-lib is linked: Airc::open(scope_home) + current_room() / subscription_set(); the daemon endpoint by its resolver, never by parsing `airc ipc-endpoint`"),
    ("continuum", "this IS the core: call the command through CommandExecutor, never re-enter through the CLI"),
    ("gh", "GitHub goes through the code/github module's client, never a CLI on PATH"),
    ("curl", "the http client (reqwest) is linked; a download is a bounded request, not a shell"),
    ("wget", "the http client (reqwest) is linked; a download is a bounded request, not a shell"),
    ("ps", "sysinfo is a dependency: System::new_with_specifics + process(pid) for rss/name/parent"),
    ("which", "walk PATH in-process: std::env::split_paths(PATH).find(|d| d.join(tool).is_file())"),
    ("tar", "the tar crate / std::fs — an archive is not a shell"),
    ("cp", "std::fs::copy / a recursive copy helper — a file is not a shell"),
];

/// Every `Command::new(` in production code on 2026-09-12 (canary 48fbbfd44). May only fall.
pub const BASELINE_SPAWN_SITES: usize = 166;

/// Per-program baselines for [`ANSWERED_BY_A_LIBRARY`] on the same day. May only fall.
pub const BASELINE_BY_PROGRAM: &[(&str, usize)] = &[
    ("airc", 6),
    ("continuum", 0),
    ("gh", 0),
    ("curl", 0),
    ("wget", 0),
    ("ps", 7),
    ("which", 3),
    ("tar", 6),
    ("cp", 4),
];

/// The program a spawn names, when it names one as a string literal. `Command::new(&bin)`
/// and friends are dynamic and read as `None` — counted in the total, not per program.
pub fn spawned_program(code: &str) -> Option<&str> {
    let i = code.find("Command::new(")? + "Command::new(".len();
    let rest = code[i..].trim_start();
    let lit = rest.strip_prefix('"')?;
    let end = lit.find('"')?;
    Some(&lit[..end])
}

pub struct SubprocessSpawn;

impl SourceRule for SubprocessSpawn {
    fn name(&self) -> &'static str {
        "subprocess-spawn"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let mut out = Vec::new();
        for (line_no, line) in file.production_lines() {
            let (code, _comment) = split_code_and_comment(line);
            if !code.contains("Command::new(") {
                continue;
            }
            out.push(Violation {
                rule: "subprocess-spawn",
                file: file.rel.clone(),
                line: line_no,
                source: code.trim().to_string(),
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_hygiene::scan;

    // what this catches: a new subprocess where a library call belongs — the
    // 2026-09-12 class, where every spawn on the node failed at once and each site
    // reported its own symptom. The total may only fall; the programs a linked
    // library answers for may only fall per program, and the failure names the
    // replacement.
    #[test]
    fn spawn_sites_only_fall_and_library_answered_programs_never_rise() {
        let sites = scan(&[&SubprocessSpawn]);
        let total = sites.len();
        let mut per: std::collections::BTreeMap<&str, Vec<String>> = Default::default();
        for v in &sites {
            if let Some(p) = spawned_program(&v.source) {
                per.entry(p).or_default().push(format!("  {}:{}  {}", v.file, v.line, v.source));
            }
        }
        let mut report = String::new();
        for (program, why) in ANSWERED_BY_A_LIBRARY {
            let n = per.get(program).map_or(0, Vec::len);
            let baseline = BASELINE_BY_PROGRAM
                .iter()
                .find(|(p, _)| p == program)
                .map_or(0, |(_, b)| *b);
            if n > baseline {
                report.push_str(&format!(
                    "\n`{program}` spawned at {n} sites (baseline {baseline}) — {why}\n{}",
                    per[program].join("\n")
                ));
            }
        }
        assert!(report.is_empty(), "a program a library already answers for gained a spawn site:{report}");
        assert!(
            total <= BASELINE_SPAWN_SITES,
            "production spawn sites rose to {total} (baseline {BASELINE_SPAWN_SITES}). A subprocess is a \
             liability the caller inherits (fds, PATH, stdio, a hang) — reach for the library, or \
             route a real subprocess through system_resources::bounded_command. Newest-looking:\n{}",
            sites.iter().rev().take(8).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
        );
        // the calibration read: per-program counts, for the next baseline edit
        eprintln!("spawn sites: {total}; by program: {:?}", per.iter().map(|(p, v)| (*p, v.len())).collect::<Vec<_>>());
    }

    #[test]
    fn the_program_is_read_from_the_literal_only() {
        assert_eq!(spawned_program(r#"let c = Command::new("airc").arg("room");"#), Some("airc"));
        assert_eq!(spawned_program(r#"tokio::process::Command::new( "ps" )"#), Some("ps"));
        assert_eq!(spawned_program("Command::new(&self.bin)"), None, "dynamic = counted, not named");
        assert_eq!(spawned_program("let x = 1;"), None);
    }
}
