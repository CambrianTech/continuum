//! The chat boundary without a model: parsing and its refusals.

use super::*;

// what this catches: the parse boundary itself (shim linked, JSON in and out, the string handed
// back and freed). Plain content under the content-only format comes back as the message content.
#[test]
fn content_only_output_parses_to_its_content() {
    let parsed = parse(r#"{"format":0}"#, "hello there", false);
    assert!(matches!(&parsed, Ok(json) if json.contains(r#""content":"hello there""#)), "{parsed:?}");
}

// what this catches: a native exception unwinding into Rust (abort) on bad input. Malformed JSON
// is a refusal naming the step.
#[test]
fn malformed_input_is_a_refusal_not_an_unwind() {
    let refused = parse("{not json", "x", false);
    assert!(matches!(&refused, Err(ChatRefusal { step: "parse", .. })), "{refused:?}");
}

// what this catches: a NUL in caller text silently truncating what the model sees.
#[test]
fn text_holding_a_nul_is_refused_before_the_native_side() {
    assert_eq!(parse(r#"{"format":0}"#, "a\0b", false), Err(ChatRefusal { step: "parse", message: "text holds a NUL byte".into() }));
}
