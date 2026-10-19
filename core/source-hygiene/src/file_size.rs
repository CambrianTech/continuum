//! A file nobody can read whole has no owner. The core held 142 files over 1,000 lines
//! (159,729 lines past that mark, the four largest 12,357 / 12,120 / 9,838 / 9,822) on canary
//! 2026-10-18; Joel: "Every file over 1000 lines must be broken up ... the monolith. It's
//! absurd." This ratchet makes that number fall and never rise: it counts the lines each
//! file carries past 1,000, summed over the crate.
//!
//! The 1,000 is the owner's line, not a measured optimum: a file of that size is still one
//! read. Splitting by OWNER (one responsibility per file) is the fix; this count only keeps
//! the direction honest. Test files count too: a 4,000-line test file is as unreadable.

use super::{SourceFile, SourceRule, Violation};

/// Lines a file may carry before every further line counts against the crate.
pub const READABLE_FILE_LINES: usize = 1_000;

/// Lines past `READABLE_FILE_LINES`, summed over the crate, measured on canary `4b6a66a6e`
/// (2026-10-18). #5164 (the M5 proving build) and #5153 (this ratchet) were each green at
/// their own heads and never ran against each other; merged in that order they left canary
/// red at 160,594. That is canary's measured state, set here so every PR is not blocked;
/// the batch split lowers it. **This number may only ever go DOWN.** A split by owner lowers
/// it; raising it is growing the monolith.
#[cfg(test)]
// 159,615 after card 0af5d5ec moved disk_eviction.rs's 1,143-line test module to
// disk_eviction/tests.rs (#![cfg(test)]): disk_eviction.rs fell from 2,426 to 1,283 lines.
// 159,463 after card 5b497852 moved lane_args.rs's 477-line test module to lane_args/tests.rs
// (#![cfg(test)]): lane_args.rs fell from 1,198 to 722 lines.
// 159,447 after card 3026b83e moved SurfaceReason and the tool-surface receipt out of the
// deliberation faculty into cognition/tool_surface.rs.
// 159,419 after card b406e901 moved ModelBinding out of the deliberation faculty into
// cognition/model_binding.rs.
// 159,391 after persona admission permit extraction (PR5176).
// 159,277 after card 573cc58a moved the cache-class eviction decisions out of
// disk_eviction/tests.rs (1,145 lines) into disk_eviction/eviction_decisions.rs.
// 158,283 after card 573cc58a moved persona_workspace.rs's 1,995-line test module to
// cognition/persona_workspace/tests.rs (#![cfg(test)]): persona_workspace.rs fell from 4,380 to 2,370 lines.
// 158,255: both card 573cc58a splits measured together on canary 16a075c71.
// 158,221 after card 32d2abbb moved the training-job directory layout and its reader out of
// modules/training_trigger.rs into genome/fine_tuning/job_dir.rs.
// 157,971 (with #5185) after card 67af4fca moved the credit stamp (CreditRole, OutcomeStamp, CapturedCredit,
// SettlementVerdict) out of persona/training_producer.rs into training_producer/stamp.rs.
// 157,645 after card a0790c4b moved persona/viewstate_rag.rs's test module to
// persona/viewstate_rag/tests.rs (#![cfg(test)]): #5015 had grown it to 1,458 lines and turned canary red.
// 157,278 (on canary 719eed2c7) after card 9f0ba854 moved the cognition/eval command's wire contract (params and
// results) out of cognition/eval.rs into cognition/eval/wire.rs.
const BASELINE_LINES_OVER: usize = 157_278;

pub struct FileSize;

impl SourceRule for FileSize {
    fn name(&self) -> &'static str {
        "file-size"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let lines = file.raw.lines().count();
        if lines <= READABLE_FILE_LINES {
            return Vec::new();
        }
        vec![Violation {
            rule: "file-size",
            file: file.rel.clone(),
            line: READABLE_FILE_LINES + 1,
            source: format!("{lines} lines: {} past {READABLE_FILE_LINES}", lines - READABLE_FILE_LINES),
        }]
    }
}

/// The excess a violation records: the number before " past".
#[cfg(test)]
fn excess(v: &Violation) -> usize {
    v.source
        .split(": ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0) // unwrap_or: `check` writes every source in this exact shape; an unparsable one counts as 0 and the shape test below catches it
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a file growing past one read without anyone deciding it should.
    #[test]
    fn a_file_past_the_readable_size_is_one_violation_carrying_its_excess() {
        let small = SourceFile::for_test("x/small.rs", &"fn a() {}\n".repeat(1_000));
        assert!(FileSize.check(&small).is_empty(), "1,000 lines is still one read");
        let big = SourceFile::for_test("x/big.rs", &"fn a() {}\n".repeat(1_250));
        let v = FileSize.check(&big);
        assert_eq!(v.len(), 1);
        assert_eq!(excess(&v[0]), 250, "{}", v[0].source);
    }

    // what this catches: the monolith growing. Every split by owner lowers the count; if
    // this fails because you lowered it, lower `BASELINE_LINES_OVER` and take the win.
    #[test]
    fn lines_past_the_readable_size_never_rise() {
        let violations = scan(&[&FileSize]);
        let total: usize = violations.iter().map(excess).sum();
        let mut worst: Vec<_> = violations.iter().map(|v| (excess(v), v.file.as_str())).collect();
        worst.sort_by(|a, b| b.0.cmp(&a.0));
        eprintln!("lines past {READABLE_FILE_LINES}: {total} across {} files (baseline {BASELINE_LINES_OVER})", violations.len());
        for (n, f) in worst.iter().take(6) {
            eprintln!("  {f}: +{n}");
        }
        assert!(
            total <= BASELINE_LINES_OVER,
            "lines past {READABLE_FILE_LINES} rose to {total} (baseline {BASELINE_LINES_OVER}). Split the \
             file you grew by owner, one responsibility per file. Never raise the baseline."
        );
        crate::assert_baseline_is_current("BASELINE_LINES_OVER", total, BASELINE_LINES_OVER);
    }
}
