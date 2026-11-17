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
        err.contains("THE CAUSE IS IN WHAT YOU SENT") && err.contains("line 2 of your content") && err.contains("//! Tree ownership"),
        "the refusal names her line, got:\n{err}"
    );
    assert!(!err.contains("most common cause is a line range"), "and never blames a range she did not use, got:\n{err}");
}
