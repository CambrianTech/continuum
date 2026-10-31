//! **The continuum home is resolved once, by `paths::continuum_home`.**
//!
//! `paths::continuum_home()` honours `$CONTINUUM_HOME` and, under `cfg(test)`, a
//! per-process temp home, so a test binary can never write the operator's records
//! (2026-09-20, the M5: a `cargo test` saved a fixture model into the live
//! served-window state, and the next boot planned 8 lanes on a box whose knee is 2).
//! Every hand-built `home_dir().join(".continuum")` bypasses both. Measured with this
//! rule's predicate on canary ae7145966: 77 such joins in production code (card 5e3a3e75).
//!
//! The count may only fall. Fix a site by resolving through
//! `crate::paths::continuum_home()` (in `continuum-base`, `crate::home::continuum_home()`; the
//! kernel's delegates to it) and joining the subdirectory onto that, never by
//! raising the baseline. It is a measured syntactic ratchet over the production half (the
//! scanner's cut, which can end a file early), not proof that every bypass is gone.

use super::{SourceFile, SourceRule, Violation};

/// The modules that own the resolution: the rule itself at the floor (`continuum-base`'s
/// `home`, which `config_env` and `secrets` use), and the kernel's `paths`, which delegates to
/// it and adds the test-build homes (card f04a437f).
const OWNERS: &[&str] = &["paths/", "continuum-base/home.rs"];

pub struct ContinuumHomeResolvedOnce;

/// The code of a line, strings intact: the scanner's one string-aware split, so a `//`
/// inside `"http://…"` is never read as a comment (Astra on #5196).
fn code(line: &str) -> &str {
    super::lexer::split_code_and_comment(line).0
}

/// Whether the code at `i` joins `.continuum` onto a path by hand, including a `.join(`
/// that ends its line with the argument on the next (a wrapped call).
fn hand_built(lines: &[&str], i: usize) -> bool {
    let mut rest = code(lines[i]);
    while let Some(at) = rest.find(".join(") {
        rest = &rest[at + ".join(".len()..];
        let arg = match rest.trim_start() {
            "" => lines.get(i + 1).map_or("", |next| code(next).trim_start()), // map_or: a .join( on a file's last line has no argument to read
            arg => arg,
        };
        if arg.starts_with("\".continuum") {
            return true;
        }
    }
    false
}

impl SourceRule for ContinuumHomeResolvedOnce {
    fn name(&self) -> &'static str {
        "continuum_home_resolved_once"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if OWNERS.iter().any(|owner| file.rel.starts_with(owner)) {
            return Vec::new();
        }
        let lines: Vec<&str> = file.production.lines().collect();
        (0..lines.len())
            .filter(|&i| hand_built(&lines, i))
            .map(|i| Violation {
                rule: self.name(),
                file: file.rel.clone(),
                line: i + 1,
                source: lines[i].trim().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a new hand-built continuum home in production code. The count of
    // existing ones may only fall; the ratchet holds the base tree's count.
    #[test]
    fn hand_built_continuum_homes_only_fall() {
        let sites = scan(&[&ContinuumHomeResolvedOnce]);
        crate::ratchet::hold("hand_built_homes", sites.len(), || {
            format!(
                "hand-built continuum homes rose. Resolve through crate::paths::continuum_home() instead:\n{}",
                sites
                    .iter()
                    .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        });
    }

    // what this catches: the predicate drifting. It must flag a join split across a call
    // chain and a joined subpath, and spare a comment, a string that only mentions the
    // directory, and the owner module.
    #[test]
    fn the_predicate_flags_joins_and_spares_mentions() {
        let one = |line: &str| hand_built(&[line], 0);
        assert!(one(
            r#"dirs::home_dir().map(|h| h.join(".continuum").join("logs"))"#
        ));
        assert!(one(r#"    .join( ".continuum/progress")"#));
        assert!(
            hand_built(
                &[
                    "    let home = home.join(",
                    r#"        ".continuum","#,
                    "    );"
                ],
                0
            ),
            "a wrapped call: the argument on the next line"
        );
        assert!(!one(r#"// built as home.join(".continuum") before"#));
        assert!(!one(r#"say("look in ~/.continuum/logs")"#));
        assert!(!one(r#"crate::paths::continuum_home()?.join("logs")"#));
        assert!(
            !one(r#"let url = "http://x/.join(\".continuum\")"; // not code"#),
            "a string holding the text is not a join"
        );
        for owner in ["paths/mod.rs", "continuum-base/home.rs"] {
            let owner = SourceFile::for_test(owner, "fn h() { home.join(\".continuum\") }\n");
            assert!(ContinuumHomeResolvedOnce.check(&owner).is_empty(), "the owner builds it: {}", owner.rel);
        }
        let floor_reader = SourceFile::for_test("continuum-base/config_env.rs", "fn h() { home.join(\".continuum\") }\n");
        assert_eq!(ContinuumHomeResolvedOnce.check(&floor_reader).len(), 1, "a floor reader is not the owner");
    }
}
