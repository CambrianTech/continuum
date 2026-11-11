//! What a tool's result proved, typed: FEEDBACK-PARITY problem 4 ("a result says what it
//! verified, and nothing more"). A tool's outcome carries evidence; each piece names the check
//! that produced it, the exact content that check read, and its verdict. It lives on the floor
//! beside [`crate::tool_effect::ToolEffect`] because the kernel (which builds outcomes) and the
//! view (which renders them) both read it and neither may import the other.
//!
//! The invariants (Codex's review of the design, 2026-11-11) are types here, not conventions:
//! evidence binds the revision and checker it ran against, so a result for older content never
//! verifies newer content; an interrupted check is `Inconclusive`, never `Failed` and never a
//! quiet `Running`; a pass proves its declared check and nothing more.

use sha2::{Digest, Sha256};

/// A kind of check. Open by adding a variant, never by prose: the renderer names every kind it
/// prints, so a new kind cannot hide inside another's wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum EvidenceKind {
    /// The content parses in its language (a syntax gate; says nothing about meaning).
    Form,
    /// The project builds with its declared checker.
    Build,
    /// The project's declared tests pass.
    Test,
    /// A declared linter passes.
    Lint,
    /// A declared command ran and exited as expected.
    Run,
    /// A reviewer's verdict on a submission.
    Review,
}

/// The exact content a check read: SHA-256 of the bytes (a file) or of the tree a project check
/// built. Two checks agree on a subject only when they read the same bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Revision([u8; 32]);

impl Revision {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut digest = [0u8; 32];
        digest.copy_from_slice(&Sha256::digest(bytes));
        Self(digest)
    }
}

/// Which checker produced the evidence, by what it ran: its command and a digest of the
/// configuration and environment it ran under. A result from a different checker, or the same
/// checker under a different configuration, is different evidence.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CheckerRun {
    pub command: Vec<String>,
    pub config: Revision,
}

/// Why a check reached no verdict. Never a failure of the content: the content is unjudged.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Inconclusive {
    /// Superseded by a newer edit and cancelled (its process custody kept, card ce191b8c).
    Cancelled,
    /// Its owner was lost (a restart) and its end was not observed.
    Lost,
    /// The check could not run for a reason outside the content (a runner's DNS, a dead checker).
    Infrastructure(String),
}

/// A check's verdict. `Running` names the task handle that will end it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Verdict {
    Passed,
    /// The checker's own finding, whole; the raw output stays reachable by [`Evidence::output`].
    Failed(String),
    Running(uuid::Uuid),
    Inconclusive(Inconclusive),
}

/// One piece of evidence: a kind of check, the content and checker it is bound to, its verdict,
/// and the handle of its raw output (stdout, stderr, exit status), never dropped.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub subject: Revision,
    pub checker: CheckerRun,
    pub verdict: Verdict,
    pub output: Option<uuid::Uuid>,
}

/// Where one kind of check stands for one exact revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Standing<'a> {
    Passed(&'a Evidence),
    Failed(&'a Evidence),
    Running(&'a Evidence),
    Inconclusive(&'a Evidence),
    /// Nothing has checked THIS revision for this kind (older results do not count).
    Unverified,
}

/// Where `kind` stands for `at`: only evidence bound to exactly `at` counts, and the last such
/// piece is the current one. A pass for an older revision leaves the newer one `Unverified`.
pub fn standing(evidence: &[Evidence], kind: EvidenceKind, at: Revision) -> Standing<'_> {
    match evidence.iter().rev().find(|e| e.kind == kind && e.subject == at) {
        None => Standing::Unverified,
        Some(e) => match &e.verdict {
            Verdict::Passed => Standing::Passed(e),
            Verdict::Failed(_) => Standing::Failed(e),
            Verdict::Running(_) => Standing::Running(e),
            Verdict::Inconclusive(_) => Standing::Inconclusive(e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(kind: EvidenceKind, subject: Revision, verdict: Verdict) -> Evidence {
        let checker = CheckerRun { command: vec!["cargo".into(), "check".into()], config: Revision::of_bytes(b"default") };
        Evidence { kind, subject, checker, verdict, output: None }
    }

    // what this catches: the M5, 2026-11-11 (Aiko): a syntax pass read as "the file is fine". A
    // Form pass says nothing about Build, which stays Unverified until a Build check of the same
    // bytes; and a pass for an older revision never verifies a newer edit (Codex invariant 1).
    #[test]
    fn a_pass_proves_its_own_check_on_its_own_revision_and_nothing_more() {
        let before = Revision::of_bytes(b"fn main() { if ok { return; } }");
        let after = Revision::of_bytes(b"fn main() { return; }");
        let all = vec![
            evidence(EvidenceKind::Form, after, Verdict::Passed),
            evidence(EvidenceKind::Build, before, Verdict::Passed),
        ];
        assert!(matches!(standing(&all, EvidenceKind::Form, after), Standing::Passed(_)));
        assert_eq!(standing(&all, EvidenceKind::Build, after), Standing::Unverified, "the old build pass is not this edit's");
        assert_eq!(standing(&all, EvidenceKind::Test, after), Standing::Unverified, "nothing ran the tests");
    }

    // what this catches: an interrupted check reported as a failure of the content, or left as a
    // Running that never ends (Codex invariant 1): it is Inconclusive, and the latest evidence for
    // the revision is the standing one.
    #[test]
    fn an_interrupted_check_is_inconclusive_and_the_latest_result_stands() {
        let at = Revision::of_bytes(b"x");
        let task = uuid::Uuid::from_u128(7);
        let mut all = vec![evidence(EvidenceKind::Test, at, Verdict::Running(task))];
        assert!(matches!(standing(&all, EvidenceKind::Test, at), Standing::Running(_)));
        all.push(evidence(EvidenceKind::Test, at, Verdict::Inconclusive(Inconclusive::Lost)));
        assert!(matches!(standing(&all, EvidenceKind::Test, at), Standing::Inconclusive(_)), "lost, not failed");
        all.push(evidence(EvidenceKind::Test, at, Verdict::Failed("1 test failed".into())));
        assert!(matches!(standing(&all, EvidenceKind::Test, at), Standing::Failed(_)));
    }
}
