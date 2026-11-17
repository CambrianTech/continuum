#![cfg(test)]
use super::*;

// what this catches (Aris on the M5, 2026-11-17, card #5953): an insert whose own text does not
// parse, refused three times with advice about a line range that ends inside a construct, though
// she was not using a range. The refusal quotes the line of her content the parser stopped on, says
// no range is involved, and leaves the file as it was.
#[test]
fn a_fault_in_her_inserted_text_is_quoted_as_hers_not_blamed_on_a_range() {
    let (dir, engine) = setup_engine();
    let original = "// a module\nmod callbacks;\n\nfn spawned() {}\n";
    std::fs::write(dir.path().join("src/actuator.rs"), original).expect("fixture written"); // JUSTIFIED: test fixture
    // Her real edit: a blank line, then `//!` (an inner doc comment, legal only at a file's top).
    let content = "\n//! Tree ownership for deploy builds.\nmod tree_owner;".to_string();
    let refused = engine
        .edit("src/actuator.rs", &EditMode::InsertAt { line: 3, content }, None)
        .expect("a refusal is a RESULT, not a transport error"); // JUSTIFIED: test fixture
    assert!(!refused.success, "an edit that does not parse is not written");
    assert_eq!(std::fs::read_to_string(dir.path().join("src/actuator.rs")).expect("fixture read"), original, "the file is unchanged"); // JUSTIFIED: test fixture
    let err = refused.error.expect("a refusal says why"); // JUSTIFIED: test fixture
    assert!(
        err.contains("THE PARSER STOPS INSIDE WHAT YOU SENT") && err.contains("line 2 of your content") && err.contains("//! Tree ownership"),
        "the refusal names her line, got:\n{err}"
    );
    assert!(!err.contains("most common cause is a line range"), "and never blames a range she did not use, got:\n{err}");
}

// what this catches (Codex's review of #6108): the same report for a line range, which must name where
// the parser stopped in her content and must not claim no range is involved, since she used one.
#[test]
fn a_line_range_whose_own_text_does_not_parse_names_her_line_without_denying_the_range() {
    let (dir, engine) = setup_engine();
    let original = "// a module\nmod callbacks;\n// the next module\nfn spawned() {}\n";
    std::fs::write(dir.path().join("src/actuator.rs"), original).expect("fixture written"); // JUSTIFIED: test fixture
    let edit = EditMode::LineRange { start_line: 3, end_line: 3, new_content: "//! the next module".to_string() };
    let refused = engine.edit("src/actuator.rs", &edit, None).expect("a refusal is a RESULT"); // JUSTIFIED: test fixture
    let err = refused.error.expect("a refusal says why"); // JUSTIFIED: test fixture
    assert!(err.contains("at line 3, which is line 1 of your content") && err.contains("//! the next module"), "got:\n{err}");
    assert!(!err.contains("No line range is involved"), "she used a range; got:\n{err}");
}

// what this catches (Codex's review of #6108): a replacement whose text already appears earlier in the
// file. The report must name where THIS edit applied (its search site), never the older identical block.
#[test]
fn a_replacement_is_located_where_it_applied_never_at_an_older_identical_block() {
    let (dir, engine) = setup_engine();
    // `//! doc` is legal at the top of a file and not after items, so only the new copy fails to parse.
    let original = "//! doc\nmod a;\n\n// note\nmod b;\n";
    std::fs::write(dir.path().join("src/blocks.rs"), original).expect("fixture written"); // JUSTIFIED: test fixture
    let edit = EditMode::SearchReplace { search: "// note".to_string(), replace: "//! doc".to_string(), all: false };
    let refused = engine.edit("src/blocks.rs", &edit, None).expect("a refusal is a RESULT"); // JUSTIFIED: test fixture
    let err = refused.error.expect("a refusal says why"); // JUSTIFIED: test fixture
    assert!(err.contains("at line 4, which is line 1 of your content"), "the edit applied at line 4, not line 1; got:\n{err}");
}
