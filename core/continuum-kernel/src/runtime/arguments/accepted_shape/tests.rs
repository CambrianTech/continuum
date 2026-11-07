use std::borrow::Cow;

use serde::Deserialize;
use serde_json::{json, Value};

use super::*;
use crate::runtime::arguments::FieldProblem;
use crate::runtime::CommandRequest;

/// `code/shell`'s params, as declared (continuum-hands `CodeShellParams`).
#[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
struct Shell {
    cmd: String,
    #[serde(default)]
    wait_ms: Option<u64>,
}

/// `work/get`'s params, as declared (continuum-mind `WorkGetParams`).
#[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
struct GetTask {
    card_id: String,
}

/// A newtype id behind a `$ref`, an optional flag and an optional ratio.
#[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
struct CardName(String);

#[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
struct Mixed {
    /// Documented, so schemars wraps the `$ref`.
    name: CardName,
    #[serde(default)]
    force: Option<bool>,
    #[serde(default)]
    ratio: Option<f64>,
}

/// The dispatch seam itself: the declared schema, the envelope's decoder.
fn dispatch<P: serde::de::DeserializeOwned + schemars::JsonSchema>(sent: &Value) -> Result<(P, Cow<'_, Value>), ArgumentsRefusal> {
    let schema = crate::sdk_codegen::input_schema::<P>();
    decode_accepting("test/accepted-shape", &schema, sent, |v| CommandRequest::<P>::decode(v).map(|r| r.params))
}

fn accepted<P: serde::de::DeserializeOwned + schemars::JsonSchema>(sent: Value) -> P {
    match dispatch::<P>(&sent) {
        Ok((params, Cow::Owned(_))) => params,
        Ok((_, Cow::Borrowed(_))) => panic!("{sent} decoded as sent; this test expects a reading"),
        Err(refusal) => panic!("{sent} refused: {refusal}"),
    }
}

fn refused_field<P: serde::de::DeserializeOwned + schemars::JsonSchema + std::fmt::Debug>(sent: Value) -> (String, FieldProblem) {
    match dispatch::<P>(&sent) {
        Err(ArgumentsRefusal::Field(field)) => (field.path.to_string(), field.problem),
        other => panic!("{sent} must refuse on a field, got {other:?}"),
    }
}

// what this catches: Sahar's captured `bash` call (Windows, 2026-11-07), the field wrapped in an
// object keyed by its own name because the collapsed offer showed `{}`; refused before this seam.
#[test]
fn sahars_shell_call_wrapped_in_its_own_name_runs() {
    let shell: Shell = accepted(json!({"cmd": {"cmd": "Get-Content C:\\work\\notes.txt"}}));
    assert_eq!(shell, Shell { cmd: "Get-Content C:\\work\\notes.txt".into(), wait_ms: None });
}

// what this catches: Sahar's `get_task` `card_id: {..}`, under its own name and under `id`.
#[test]
fn sahars_get_task_card_id_object_runs() {
    let own: GetTask = accepted(json!({"card_id": {"card_id": "08ece9e8"}}));
    let other: GetTask = accepted(json!({"card_id": {"id": "08ece9e8"}}));
    assert_eq!(own, GetTask { card_id: "08ece9e8".into() });
    assert_eq!(other, own);
}

// what this catches: exact scalar text in either direction, the wrapped-and-text case, and an
// `Option<T>` / documented `$ref` newtype still counting as its one declared scalar.
#[test]
fn exact_scalar_readings_convert() {
    let shell: Shell = accepted(json!({"cmd": "ls", "wait_ms": "30000"}));
    assert_eq!(shell.wait_ms, Some(30000));
    let wrapped: Shell = accepted(json!({"cmd": "ls", "wait_ms": {"wait_ms": "5"}}));
    assert_eq!(wrapped.wait_ms, Some(5));
    let card: GetTask = accepted(json!({"card_id": 8}));
    assert_eq!(card.card_id, "8");
    let mixed: Mixed = accepted(json!({"name": {"name": "spec"}, "force": "true", "ratio": "0.5"}));
    assert_eq!(mixed, Mixed { name: CardName("spec".into()), force: Some(true), ratio: Some(0.5) });
}

// what this catches: a call that decodes as sent is never rewritten (the reading runs only after
// the decoder refused), so a valid call costs no copy and no probe.
#[test]
fn a_call_that_decodes_is_untouched() {
    let sent = json!({"cmd": "ls", "wait_ms": 10});
    assert!(matches!(dispatch::<Shell>(&sent), Ok((_, Cow::Borrowed(_)))));
}

// what this catches: every shape with more than one plausible reading still refuses, with the
// decoder's own typed refusal naming the field.
#[test]
fn ambiguous_shapes_still_refuse() {
    for sent in [
        json!({"cmd": {"cmd": "ls", "cwd": "C:\\"}}), // two keys
        json!({"cmd": {"command": {"text": "ls"}}}), // one key holding an object
        json!({"cmd": {"command": 5}}),               // another name, not the declared kind
        json!({"cmd": ["ls"]}),                       // a list where a string is declared
    ] {
        assert_eq!(refused_field::<Shell>(sent).0, "cmd");
    }
    for loose in ["+5", "05", " 5", "5 lines", "1e3", "-1"] {
        let (field, _) = refused_field::<Shell>(json!({"cmd": "ls", "wait_ms": loose}));
        assert_eq!(field, "wait_ms", "`{loose}` is not exactly a u64");
    }
    let (field, _) = refused_field::<Mixed>(json!({"name": "spec", "force": 1}));
    assert_eq!(field, "force", "0/1 for a boolean is a guess");
}

// what this catches: a read call that still does not decode is refused on the field that is
// still wrong, not on the one the reading fixed, so her next move is the right field.
#[test]
fn a_read_call_refuses_on_the_field_still_wrong() {
    let (field, problem) = refused_field::<Shell>(json!({"cmd": {"cmd": "ls"}, "wait_ms": [1]}));
    assert_eq!(field, "wait_ms");
    assert!(matches!(problem, FieldProblem::WrongType { .. }), "{problem:?}");
}

// what this catches: each reading is recorded with its property, from-shape and to-shape, the
// data the offer's owner fixes the source from.
#[test]
fn readings_name_property_and_shapes() {
    let schema = crate::sdk_codegen::input_schema::<Shell>();
    let read = read(&schema, &json!({"cmd": {"cmd": "ls"}, "wait_ms": "7"})).expect("two readings"); // test: no reading here is the failure being caught
    let mut shapes: Vec<(String, Reading, JsonKind, Scalar)> =
        read.shapes.into_iter().map(|s| (s.property, s.reading, s.from, s.to)).collect();
    shapes.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        shapes,
        vec![
            ("cmd".into(), Reading::WrappedInOwnName, JsonKind::Object, Scalar::String),
            ("wait_ms".into(), Reading::TextAsScalar, JsonKind::String, Scalar::Integer),
        ]
    );
}
