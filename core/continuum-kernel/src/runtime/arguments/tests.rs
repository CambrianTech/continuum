use super::*;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Mode {
    Replace {
        #[serde(alias = "old_string")]
        search: String,
        replace: String,
    },
    Append {
        content: String,
    },
}

#[derive(Debug, Deserialize, PartialEq)]
struct Edit {
    file_path: String,
    mode: Mode,
    #[serde(default)]
    lines: Option<Vec<u32>>,
}

fn every(_: &str) -> bool {
    true
}

fn refused(value: serde_json::Value) -> FieldRefusal {
    match decode::<Edit>(&value, every) {
        Err(ArgumentsRefusal::Field(refusal)) => refusal,
        other => panic!("expected a field refusal, got {other:?}"),
    }
}

// what this catches: the decoder accepting less than serde_json does on a valid call (a nested
// internally tagged enum, a serde alias, an optional list, null for an option), which would
// refuse calls before dispatch that the command itself takes.
#[test]
fn a_valid_call_decodes_as_serde_json_decodes_it() {
    let value = json!({ "file_path": "a.rs", "mode": { "type": "replace", "old_string": "x", "replace": "y" }, "lines": [1, 2] });
    let ours = decode::<Edit>(&value, every).expect("a valid call decodes"); // JUSTIFIED: the invariant under test
    let theirs: Edit = serde_json::from_value(value).expect("serde_json decodes it"); // JUSTIFIED: the invariant under test
    assert_eq!(ours, theirs);
    let with_null = json!({ "file_path": "a.rs", "mode": { "type": "append", "content": "z" }, "lines": null });
    assert_eq!(decode::<Edit>(&with_null, every).ok().map(|e| e.lines), Some(None));
}

// what this catches: a nested failure reported without its path, or as message text: the
// element of a list names its index, a missing field inside a tagged enum names the enum's
// field and the inner field, and an unknown tag lists the tags it may be.
#[test]
fn a_nested_failure_names_its_path_and_problem() {
    let bad_index = refused(json!({ "file_path": "a.rs", "mode": { "type": "append", "content": "z" }, "lines": [1, "two"] }));
    assert_eq!(bad_index.path.0, vec![Segment::Key("lines".into()), Segment::Index(1)]);
    assert!(matches!(bad_index.problem, FieldProblem::WrongType { sent: JsonKind::String, .. }), "{bad_index:?}");

    let inner_missing = refused(json!({ "file_path": "a.rs", "mode": { "type": "replace", "search": "x" } }));
    assert_eq!(inner_missing.path.to_string(), "mode.replace");
    assert!(matches!(inner_missing.problem, FieldProblem::Missing { .. }), "{inner_missing:?}");

    let unknown_tag = refused(json!({ "file_path": "a.rs", "mode": { "type": "rewrite" } }));
    match unknown_tag.problem {
        FieldProblem::UnknownVariant { variant, expected } => {
            assert_eq!(variant, "rewrite");
            assert_eq!(expected, &["replace", "append"]);
        }
        other => panic!("expected an unknown variant, got {other:?}"),
    }
}

// what this catches: the root filter leaking, so the envelope's keys reach a command's params
// (or the reverse), the property that lets both decode from one object.
#[test]
fn the_root_filter_reads_only_the_keys_it_keeps() {
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Only {
        file_path: String,
    }
    let value = json!({ "file_path": "a.rs", "sessionId": "s" });
    assert!(decode::<Only>(&value, every).is_err(), "deny_unknown_fields sees the key unfiltered");
    let kept = decode::<Only>(&value, |k| k != "sessionId").expect("filtered, the key never reaches it"); // JUSTIFIED: the invariant under test
    assert_eq!(kept.file_path, "a.rs");
}

// what this catches: a cut call offered a retry. The same call cuts at the same limit (83
// consecutive cuts on the M5), so its refusal must say its way out is not a retry.
#[test]
fn a_cut_is_never_answered_with_a_retry() {
    let cut = ArgumentsRefusal::Unparsed(UnparsedArguments::Cut { raw: "{\"content\":\"x".into() });
    assert!(!cut.retries());
    let malformed = ArgumentsRefusal::Unparsed(UnparsedArguments::Malformed { parse_error: "eof".into(), raw: "{".into() });
    assert!(malformed.retries());
    let unexplained = ArgumentsRefusal::Unparsed(UnparsedArguments::Unexplained { raw: "{}".into() });
    assert!(!unexplained.retries(), "an unreported finish is the transport's fault: no retry answers it");
    assert!(malformed.to_string().contains('{'), "the raw text she sent is shown whole");
}
