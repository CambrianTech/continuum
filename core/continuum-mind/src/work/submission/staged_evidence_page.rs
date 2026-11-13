//! `work/submission`'s staged evidence answers the question a reader acts on, sized by
//! what it shows. Every answer carries a summary of every row (how many match the
//! submission's claim, how many do not, how many predate it) and ONE PAGE of rows in full,
//! in staged order: by default the rows that do NOT match (those need a decision), or every
//! row with `evidence_rows: "all"`. `evidence_offset` / `evidence_limit` choose the page; the
//! answer's `note` says exactly how many rows it left out, their measured size, and the
//! parameters that read the next page.
//!
//! With no `evidence_limit` the answer is the SUMMARY alone, with zero rows, and its note names
//! exactly how to ask for rows (`evidence_rows="not_matching_claim" evidence_offset=0
//! evidence_limit=<n>`). Rows come only on an explicit `evidence_limit`, and the answer then
//! holds exactly what she asked for: bounded by construction, no constant and no clip. A share
//! of the whole evidence is not a bound (it grows with the evidence: Cormac's all-mismatched
//! card on #5504). Once her measured page budget reaches this verb (card 441af5ec), the default
//! can carry a first page sized by it.
//!
//! This is the TOOL answering a smaller question, never a clip (plan goal 7): Aris's one
//! `include_staged_evidence` answer was 340,832 chars, 254,799 of them 578 rows of which 576
//! matched the claim (M5, 2026-10-26), more than her whole slot, so every later act overflowed.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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

/// Which staged rows a page is taken from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/work/StagedEvidenceRows.ts")]
pub enum StagedEvidenceRows {
    #[default]
    NotMatchingClaim,
    All,
}

impl StagedEvidenceRows {
    fn admits(self, row: &StagedCreditEvidence) -> bool {
        match self {
            Self::NotMatchingClaim => !row.matches_submission_claim,
            Self::All => true,
        }
    }

    fn named(self) -> &'static str {
        match self {
            Self::NotMatchingClaim => "rows that do not match this submission's claim",
            Self::All => "staged rows",
        }
    }

    fn param(self) -> &'static str {
        match self {
            Self::NotMatchingClaim => "evidence_rows=\"not_matching_claim\"",
            Self::All => "evidence_rows=\"all\"",
        }
    }
}

/// The page an answer shows: its rows, where it starts among them, and the limit asked for
/// (`None`: the summary alone, no rows).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/work/StagedEvidenceSelection.ts")]
pub struct StagedEvidenceSelection {
    pub rows: StagedEvidenceRows,
    pub offset: usize,
    pub limit: Option<usize>,
}

/// The page a reader asked for, from `evidence_rows` / `evidence_offset` / `evidence_limit`.
pub type EvidenceRequest = StagedEvidenceSelection;

impl StagedEvidenceSelection {
    pub fn from_params(rows: Option<StagedEvidenceRows>, offset: Option<usize>, limit: Option<usize>) -> Self {
        Self { rows: rows.unwrap_or_default(), offset: offset.unwrap_or_default(), limit } // unwrap_or_default: absent = the mismatches, from their first row
    }
}

/// The staged-evidence answer: a summary of all rows, one page of rows in full, and what was
/// left out with how to read it.
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

/// Serialized size of one row as the answer carries it.
fn chars_of(row: &StagedCreditEvidence) -> Result<usize, serde_json::Error> {
    Ok(serde_json::to_string(row)?.chars().count()) // measures the JSON the tool answer carries to the mind
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
        let selected = rows.iter().filter(|row| request.rows.admits(row)).count();
        let (mut shown, mut seen, mut omitted_chars) = (Vec::new(), 0usize, 0usize);
        for row in rows {
            let admitted = request.rows.admits(&row);
            let on_page = admitted
                && seen >= request.offset
                && request.limit.is_some_and(|limit| seen - request.offset < limit);
            if admitted {
                seen += 1;
            }
            if on_page {
                shown.push(row);
            } else {
                omitted_chars += chars_of(&row)?;
            }
        }
        let omitted_rows = summary.total_rows - shown.len();
        let note = Self::note(&summary, request, selected, shown.len(), omitted_rows, omitted_chars);
        Ok(Self { summary, selection: request, rows: shown, omitted_rows, omitted_chars, note })
    }

    fn note(
        summary: &StagedEvidenceSummary,
        request: EvidenceRequest,
        selected: usize,
        shown: usize,
        omitted: usize,
        omitted_chars: usize,
    ) -> String {
        let total = summary.total_rows;
        if total == 0 {
            return "no rows are staged for this card, so nothing is shown and nothing is omitted".into();
        }
        let counts = format!(
            "{total} staged rows: {} match this submission's claim, {} do not; {} predate the submission",
            summary.matching_claim, summary.not_matching_claim, summary.predating_submission
        );
        let named = request.rows.named();
        let param = request.rows.param();
        let Some(limit) = request.limit else {
            return format!(
                "{counts}. No rows shown: this answer is the summary ({omitted_chars} chars of rows \
                 omitted). For rows, ask with evidence_limit: {param} evidence_offset={} \
                 evidence_limit=<n> pages the {selected} {named}; evidence_rows=\"all\" pages every row.",
                request.offset
            );
        };
        let shown_part = if shown == 0 {
            format!("shown: none of the {selected} {named} (offset {} is past the last)", request.offset)
        } else {
            format!(
                "shown in full: {named} {}-{} of {selected} in staged order (evidence_limit={limit})",
                request.offset + 1,
                request.offset + shown
            )
        };
        let next = request.offset.saturating_add(shown);
        let omitted_part = match () {
            _ if omitted == 0 => "nothing omitted".to_owned(),
            _ if next < selected => format!(
                "omitted {omitted} rows ({omitted_chars} chars); next page: {param} evidence_offset={next} \
                 evidence_limit={limit}{}",
                if request.rows == StagedEvidenceRows::All { "" } else { "; every row: evidence_rows=\"all\"" }
            ),
            _ => format!(
                "omitted {omitted} rows ({omitted_chars} chars); read them with evidence_rows=\"all\" and \
                 evidence_offset / evidence_limit"
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

    fn size(value: &impl Serialize) -> Result<usize, serde_json::Error> {
        Ok(serde_json::to_string(value)?.chars().count())
    }

    fn at_ms(page: &StagedEvidencePage) -> Vec<u64> {
        page.rows.iter().map(|r| r.staged_at_ms).collect()
    }

    // what this catches: the default answer returning rows at all (Aris's 254,799-char
    // evidence, more than her slot): with no evidence_limit it is the summary and the note.
    #[test]
    fn the_default_answer_is_the_summary_and_names_how_to_ask_for_rows() -> Result<(), serde_json::Error> {
        let rows = aris_rows();
        let whole = size(&rows)?;
        let page = StagedEvidencePage::answer(rows, EvidenceRequest::from_params(None, None, None))?;
        assert_eq!((page.summary.total_rows, page.summary.matching_claim, page.summary.not_matching_claim), (578, 576, 2));
        assert!(page.rows.is_empty() && page.omitted_rows == 578);
        assert_eq!(page.omitted_chars, whole - 579, "the omitted size is measured, not guessed");
        assert!(size(&page)? < 2_048, "a summary, not the evidence: {} chars", size(&page)?);
        let ask = "evidence_rows=\"not_matching_claim\" evidence_offset=0 evidence_limit=<n>";
        assert!(page.note.contains("576 match") && page.note.contains("2 do not") && page.note.contains(ask), "{}", page.note);
        Ok(())
    }

    // what this catches: Cormac's case on #5504. A reclaimed card whose rows all belong to an
    // earlier claim answered every mismatch in full; the default stays the summary whatever
    // the evidence's size.
    #[test]
    fn all_rows_mismatched_default_to_the_summary_alone() -> Result<(), serde_json::Error> {
        let rows: Vec<_> = (0..578u64).map(|at| row(at, false)).collect();
        let page = StagedEvidencePage::answer(rows, EvidenceRequest::from_params(None, None, None))?;
        assert!(page.rows.is_empty() && page.summary.not_matching_claim == 578);
        assert!(size(&page)? < 2_048, "{} chars", size(&page)?);
        assert!(page.note.contains("pages the 578 rows that do not match"), "{}", page.note);
        Ok(())
    }

    // what this catches: an explicit limit answering other than exactly what was asked.
    #[test]
    fn an_explicit_limit_returns_exactly_those_mismatches() -> Result<(), serde_json::Error> {
        let page = StagedEvidencePage::answer(aris_rows(), EvidenceRequest::from_params(None, Some(0), Some(10)))?;
        assert_eq!(at_ms(&page), vec![100, 400]);
        assert_eq!(page.omitted_rows, 576);
        assert!(page.note.contains("nothing omitted") || page.note.contains("evidence_rows=\"all\""), "{}", page.note);
        let second = StagedEvidencePage::answer(aris_rows(), EvidenceRequest::from_params(None, Some(1), Some(1)))?;
        assert_eq!(at_ms(&second), vec![400], "an offset pages within the mismatches");
        Ok(())
    }

    // what this catches: a page that returns other rows than asked, or a note that does not
    // name the next page.
    #[test]
    fn an_explicit_page_returns_exactly_those_rows_and_names_the_next() -> Result<(), serde_json::Error> {
        let all = Some(StagedEvidenceRows::All);
        let page = StagedEvidencePage::answer(aris_rows(), EvidenceRequest::from_params(all, Some(10), Some(5)))?;
        assert_eq!(at_ms(&page), vec![10, 11, 12, 13, 14]);
        assert_eq!(page.omitted_rows, 573);
        assert!(page.note.contains("11-15") && page.note.contains("evidence_offset=15"), "{}", page.note);
        let last = StagedEvidencePage::answer(aris_rows(), EvidenceRequest::from_params(all, Some(575), Some(50)))?;
        assert_eq!(last.rows.len(), 3);
        let past = StagedEvidencePage::answer(aris_rows(), EvidenceRequest::from_params(all, Some(900), Some(5)))?;
        assert!(past.rows.is_empty() && past.note.contains("past the last"), "{}", past.note);
        Ok(())
    }

    // what this catches: an empty store read as a failure or as rows hidden from her.
    #[test]
    fn zero_rows_read_honestly() -> Result<(), serde_json::Error> {
        let page = StagedEvidencePage::answer(Vec::new(), EvidenceRequest::from_params(None, None, None))?;
        assert_eq!(page.summary.total_rows, 0);
        assert!(page.rows.is_empty() && page.omitted_rows == 0);
        assert!(page.note.contains("no rows are staged"), "{}", page.note);
        Ok(())
    }
}
