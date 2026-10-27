//! `work/submission`'s staged evidence answers the question a reader acts on, sized by
//! what it shows. Its default answer is a summary of every row (how many match the
//! submission's claim, how many do not, how many predate it) plus, in full, only the rows
//! that do NOT match: those are the ones that need a decision. Every other row is reachable
//! as a page in staged order with `evidence_offset` / `evidence_limit`, and the answer's
//! `note` says exactly how many rows it left out, their size, and the parameters that read them.
//!
//! This is the TOOL answering a smaller question, never a clip (plan goal 7): Aris's one
//! `include_staged_evidence` answer was 340,832 chars, 254,799 of them 578 rows of which 576
//! matched the claim (M5, 2026-10-26), more than her whole slot, so every later act overflowed.
//! A mind's view never cuts her newest result; the tool returns a different, smaller answer.

use serde::Serialize;
use ts_rs::TS;

use crate::persona::training_producer::reviewed::StagedCreditEvidence;

/// The counts over EVERY staged row for the card, whichever rows the answer shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/work/StagedEvidenceSummary.ts")]
pub struct StagedEvidenceSummary {
    pub total_rows: usize,
    pub matching_claim: usize,
    pub not_matching_claim: usize,
    pub predating_submission: usize,
    pub after_submission: usize,
    pub generations: usize,
}

/// Which rows an answer shows in full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/work/StagedEvidenceSelection.ts")]
pub enum StagedEvidenceSelection {
    /// The default: every row that does not match the submission's claim.
    NotMatchingClaim,
    /// Rows `offset..offset + limit` in staged order, as asked.
    Page { offset: usize, limit: usize },
}

/// The staged-evidence answer: a summary of all rows, the selected rows in full, and what
/// was left out with how to read it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/work/StagedEvidencePage.ts")]
pub struct StagedEvidencePage {
    pub summary: StagedEvidenceSummary,
    pub selection: StagedEvidenceSelection,
    pub rows: Vec<StagedCreditEvidence>,
    pub omitted_rows: usize,
    /// Serialized size of the omitted rows, measured: what reading them all would cost.
    pub omitted_chars: usize,
    pub note: String,
}

/// The page a reader asked for, from `evidence_offset` / `evidence_limit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceRequest {
    NotMatchingClaim,
    Page { offset: usize, limit: usize },
}

impl EvidenceRequest {
    /// Neither parameter: the default answer. A limit pages from the offset (0 when absent).
    /// An offset with no limit names no size, so it is refused with how to ask.
    pub fn from_params(offset: Option<usize>, limit: Option<usize>) -> Result<Self, String> {
        match (offset, limit) {
            (None, None) => Ok(Self::NotMatchingClaim),
            (None, Some(limit)) => Ok(Self::Page { offset: 0, limit }),
            (Some(offset), Some(limit)) => Ok(Self::Page { offset, limit }),
            (Some(_), None) => Err("evidence_offset pages the staged rows in staged order and needs evidence_limit, \
                 the number of rows to return; the default answer's summary and note state how many \
                 rows exist and their size"
                .into()),
        }
    }
}

/// Serialized size of `rows` as the answer would carry them, one row at a time.
fn chars_of(rows: &[StagedCreditEvidence]) -> Result<usize, serde_json::Error> {
    rows.iter().try_fold(0, |sum, row| Ok(sum + serde_json::to_string(row)?.chars().count())) // measures the JSON the tool answer carries to the mind
}

impl StagedEvidencePage {
    /// `rows` (every staged row for the card, in staged order) answered as `request` asks.
    pub fn answer(rows: Vec<StagedCreditEvidence>, request: EvidenceRequest) -> Result<Self, serde_json::Error> {
        let matching_claim = rows.iter().filter(|row| row.matches_submission_claim).count();
        let predating_submission = rows.iter().filter(|row| row.predates_submission).count();
        let summary = StagedEvidenceSummary {
            total_rows: rows.len(),
            matching_claim,
            not_matching_claim: rows.len() - matching_claim,
            predating_submission,
            after_submission: rows.len() - predating_submission,
            generations: rows.iter().map(|row| row.generation_count).sum(),
        };
        let total = rows.len();
        let (selection, shown, omitted): (_, Vec<_>, Vec<_>) = match request {
            EvidenceRequest::NotMatchingClaim => {
                let (shown, omitted) = rows.into_iter().partition(|row| !row.matches_submission_claim);
                (StagedEvidenceSelection::NotMatchingClaim, shown, omitted)
            }
            EvidenceRequest::Page { offset, limit } => {
                let end = offset.saturating_add(limit).min(total);
                let start = offset.min(total);
                let mut omitted = rows;
                let tail = omitted.split_off(end);
                let shown = omitted.split_off(start);
                omitted.extend(tail);
                (StagedEvidenceSelection::Page { offset, limit }, shown, omitted)
            }
        };
        let omitted_chars = chars_of(&omitted)?;
        let note = Self::note(&summary, selection, shown.len(), omitted.len(), omitted_chars);
        Ok(Self { summary, selection, rows: shown, omitted_rows: omitted.len(), omitted_chars, note })
    }

    fn note(summary: &StagedEvidenceSummary, selection: StagedEvidenceSelection, shown: usize, omitted: usize, omitted_chars: usize) -> String {
        let total = summary.total_rows;
        if total == 0 {
            return "no rows are staged for this card, so nothing is shown and nothing is omitted".into();
        }
        let counts = format!(
            "{total} staged rows: {} match this submission's claim, {} do not; {} predate the submission",
            summary.matching_claim, summary.not_matching_claim, summary.predating_submission
        );
        let shown_part = match selection {
            StagedEvidenceSelection::NotMatchingClaim => format!("shown in full: the {shown} that do not match"),
            StagedEvidenceSelection::Page { offset, .. } if shown == 0 => {
                format!("shown: none, offset {offset} is past the last row")
            }
            StagedEvidenceSelection::Page { offset, .. } => {
                format!("shown in full: rows {}-{} in staged order", offset + 1, offset + shown)
            }
        };
        let omitted_part = match selection {
            _ if omitted == 0 => "nothing omitted".to_owned(),
            StagedEvidenceSelection::Page { offset, .. } if offset.saturating_add(shown) < total => format!(
                "omitted {omitted} rows ({omitted_chars} chars); next page: evidence_offset={} with evidence_limit",
                offset + shown
            ),
            _ => format!(
                "omitted {omitted} rows ({omitted_chars} chars); read them in staged order with \
                 evidence_offset and evidence_limit"
            ),
        };
        format!("{counts}. {shown_part}; {omitted_part}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn row(at: u64, matches: bool) -> StagedCreditEvidence {
        StagedCreditEvidence {
            revision_id: Uuid::new_v4(),
            claim_id: Some(Uuid::new_v4()),
            owner: Some(Uuid::new_v4()),
            staged_at_ms: at,
            generation_count: 1,
            generation_request_ids: vec![Uuid::new_v4().to_string()],
            matches_submission_claim: matches,
            predates_submission: true,
        }
    }

    /// Aris's shape (M5, 2026-10-26): 578 rows, 2 of which do not match the claim.
    fn aris_rows() -> Vec<StagedCreditEvidence> {
        (0..578u64).map(|at| row(at, at != 100 && at != 400)).collect()
    }

    // what this catches: the default answer returning every row (Aris's 254,799-char
    // evidence, more than her slot), or losing the rows she must act on.
    #[test]
    fn the_default_answer_is_the_summary_and_the_mismatches_in_full() -> Result<(), serde_json::Error> {
        let rows = aris_rows();
        let whole = chars_of(&rows)?;
        let page = StagedEvidencePage::answer(rows, EvidenceRequest::NotMatchingClaim)?;
        assert_eq!(page.summary.total_rows, 578);
        assert_eq!(page.summary.matching_claim, 576);
        assert_eq!(page.summary.not_matching_claim, 2);
        assert_eq!(page.rows.iter().map(|r| r.staged_at_ms).collect::<Vec<_>>(), vec![100, 400]);
        assert_eq!(page.omitted_rows, 576);
        assert_eq!(page.omitted_chars + chars_of(&page.rows)?, whole, "the omitted size is measured, not guessed");
        let answer = serde_json::to_string(&page)?.chars().count();
        assert!(answer * 50 < whole, "answer {answer} chars vs {whole} for every row");
        assert!(page.note.contains("omitted 576 rows") && page.note.contains("evidence_offset"), "{}", page.note);
        Ok(())
    }

    // what this catches: a page that returns other rows than asked, or a note that does not
    // name the next page.
    #[test]
    fn an_explicit_page_returns_exactly_those_rows_and_names_the_next() -> Result<(), serde_json::Error> {
        let request = EvidenceRequest::from_params(Some(10), Some(5));
        assert_eq!(request, Ok(EvidenceRequest::Page { offset: 10, limit: 5 }));
        let page = StagedEvidencePage::answer(aris_rows(), EvidenceRequest::Page { offset: 10, limit: 5 })?;
        assert_eq!(page.rows.iter().map(|r| r.staged_at_ms).collect::<Vec<_>>(), vec![10, 11, 12, 13, 14]);
        assert_eq!(page.omitted_rows, 573);
        assert!(page.note.contains("rows 11-15") && page.note.contains("evidence_offset=15"), "{}", page.note);
        let last = StagedEvidencePage::answer(aris_rows(), EvidenceRequest::Page { offset: 575, limit: 50 })?;
        assert_eq!(last.rows.len(), 3);
        let past = StagedEvidencePage::answer(aris_rows(), EvidenceRequest::Page { offset: 900, limit: 5 })?;
        assert!(past.rows.is_empty() && past.note.contains("past the last row"), "{}", past.note);
        assert!(EvidenceRequest::from_params(Some(3), None).is_err(), "an offset with no size is refused");
        Ok(())
    }

    // what this catches: an empty store read as a failure or as rows hidden from her.
    #[test]
    fn zero_rows_read_honestly() -> Result<(), serde_json::Error> {
        let page = StagedEvidencePage::answer(Vec::new(), EvidenceRequest::NotMatchingClaim)?;
        assert_eq!(page.summary.total_rows, 0);
        assert!(page.rows.is_empty() && page.omitted_rows == 0);
        assert!(page.note.contains("no rows are staged"), "{}", page.note);
        Ok(())
    }
}
