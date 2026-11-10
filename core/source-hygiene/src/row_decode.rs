//! **A stored row is decoded by its one owner, `DataRecord::decode`.**
//!
//! Every reader of a `DataRecord` once called `serde_json::from_value(row.data)` itself (15
//! production sites). A column an entity gained later is NULL in every older row, which that call
//! reads as a present null, so a `#[serde(default)]` field failed on all of them: on the M5
//! (2026-11-10) the training trigger's recovery died every 30 s on 380 rows and no resident's
//! in-place learning ran. The owner (`core/continuum-kernel/src/orm/record_decode.rs`) reads such
//! a NULL as absent. This rule finds a production line that decodes a record's `.data` by hand; it
//! read 0 when the owner landed, so it is a hard zero.

use super::{sites, SourceFile, SourceRule, Violation};

/// The owner, as a repository path.
pub const OWNER: &str = "core/continuum-kernel/src/orm/record_decode.rs";

pub struct RowDecode;

/// Whether `code` decodes a value's `.data` field with `serde_json::from_value` by hand.
pub fn decodes_a_row_by_hand(code: &str) -> bool {
    code.match_indices("from_value(").any(|(at, call)| {
        let args = &code[at + call.len()..];
        args.split_once(')').is_some_and(|(inside, _)| inside.trim().ends_with(".data"))
    })
}

impl SourceRule for RowDecode {
    fn name(&self) -> &'static str {
        "row_decode_outside_the_owner"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if file.repo == OWNER {
            return Vec::new();
        }
        sites(file, self.name(), |code| decodes_a_row_by_hand(code).then(|| "a row decoded by hand".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a reader decoding a stored row itself again, where a column the row
    // predates fails it. It is zero; it stays zero.
    #[test]
    fn rows_are_decoded_only_by_their_owner() {
        crate::ratchet::zero("row_decode_outside_the_owner", &crate::scan(&[&RowDecode]), || {
            "Read a stored row with `record.decode()` (DataRecord::decode, orm/record_decode.rs): it \
             reads a column the row predates as absent, so a field's default answers it."
                .to_string()
        });
    }

    // what this catches: the predicate missing the shape it exists for, or flagging other decodes.
    #[test]
    fn a_hand_decode_of_data_is_a_site_and_other_decodes_are_not() {
        assert!(decodes_a_row_by_hand("let row: Submission = serde_json::from_value(row.data)?;"));
        assert!(decodes_a_row_by_hand("Ok(serde_json::from_value( record.data )?)"));
        assert!(!decodes_a_row_by_hand("let p: Params = serde_json::from_value(params)?;"));
        assert!(!decodes_a_row_by_hand("serde_json::from_value(row.data_version)"), "a different field");
        assert!(!decodes_a_row_by_hand("let x = record.decode()?;"));
    }
}
