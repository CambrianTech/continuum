//! **Tenant neutrality: our own org, repo, and room names never enter production
//! code.**
//!
//! Joel, 2026-09-05: *"god I hope you haven't coded Cambriantech string or continuum
//! rooms or orgs. These are ours only… This is a repo for other people and their orgs
//! and ideas."* Continuum is installed by strangers for THEIR orgs. The org room
//! derives from the checkout's git remote (`airc_lib::JoinContext::from_cwd`), the
//! project room is an activity the user spawns under it, and cards name the repo the
//! user's checkout points at. A literal `CambrianTech` in a code path is a product
//! that only works for us — and the first thing a stranger reads.
//!
//! Ratchet doctrine (same as [`super::unwrap_justification`]): the baseline may only
//! ever go DOWN. The survivors at the day this landed are the product's OWN upstream
//! URLs (the airc installer and the model-card attribution link), which name where
//! the software comes from, not whose org is using it.

use super::{split_code_and_comment, SourceFile, SourceRule, Violation};

/// The tenant identities that must never appear in production code. Lower-case;
/// matched case-insensitively against CODE (string literals included — a URL is
/// code), never against comments.
const TENANT_TOKENS: &[&str] = &["cambriantech", "joel"];

/// Production occurrences when this guard landed (2026-09-05): `airc/discovery.rs`
/// (the airc installer URL) and `forge/hf_publisher.rs` (the model-card attribution
/// link). Both name the product's upstream. A NEW one has to argue.
const BASELINE_TENANT_LINES: usize = 2;

pub struct NoTenantIdentityInProduction;

impl SourceRule for NoTenantIdentityInProduction {
    fn name(&self) -> &'static str {
        "no_tenant_identity_in_production_code"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        // The guard's own token table is the one legitimate place for the word.
        if file.rel == "source_hygiene/tenant_neutrality.rs" {
            return Vec::new();
        }
        file.production_lines()
            .filter(|(_, l)| {
                let (code, _comment) = split_code_and_comment(l);
                let lower = code.to_ascii_lowercase();
                TENANT_TOKENS.iter().any(|t| lower.contains(t))
            })
            .map(|(line, l)| Violation {
                rule: self.name(),
                file: file.rel.clone(),
                line,
                source: l.trim().to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_hygiene::scan;

    // what this catches: our org name creeping into a code path — a default room,
    // a repo key, an example the runtime actually uses. If this fails on your
    // change: derive the identity (git remote, spawned activity, the user's
    // checkout) or use a placeholder (`owner/name`, `your-org`).
    #[test]
    fn tenant_identity_in_production_code_never_rises() {
        let violations = scan(&[&NoTenantIdentityInProduction]);
        assert!(
            violations.len() <= BASELINE_TENANT_LINES,
            "production lines naming our own org rose to {} (baseline {BASELINE_TENANT_LINES}).\n\
             This repo is for other people and their orgs: derive the identity from the \
             git remote or the spawned activity, never a literal.\nOffenders:\n{}",
            violations.len(),
            violations
                .iter()
                .map(|v| format!("  {}:{} — {}", v.file, v.line, v.source))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // what this catches: the predicate erring in either direction — a comment
    // quoting the name is prose, a string literal carrying it is code.
    #[test]
    fn predicate_matches_code_not_prose() {
        let rule = NoTenantIdentityInProduction;
        let code = SourceFile::for_test("x/y.rs", r#"    let repo = "CambrianTech/thing";"#);
        let prose = SourceFile::for_test("x/y.rs", "    // CambrianTech is where this came from");
        let mixed = SourceFile::for_test("x/y.rs", r#"    let a = 1; // e.g. cambriantech"#);
        assert_eq!(rule.check(&code).len(), 1, "a literal in code is a violation");
        assert_eq!(rule.check(&prose).len(), 0, "a comment is prose");
        assert_eq!(rule.check(&mixed).len(), 0, "the name in a trailing comment is prose");
    }
}
