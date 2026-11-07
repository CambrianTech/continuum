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

/// `work/get`'s params (continuum-mind `WorkGetParams`), its id behind a newtype `$ref`: the same
/// declared string, and an id is never a bare `String` field (`loose_id_guard`).
#[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
struct CardRef(String);

#[derive(Debug, Deserialize, PartialEq, schemars::JsonSchema)]
struct GetTask {
    card_id: CardRef,
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
    assert_eq!(own, GetTask { card_id: CardRef("08ece9e8".into()) });
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
    assert_eq!(card.card_id, CardRef("8".into()));
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

/// Whole tool calls written inside an argument, from the M5's captured calls (2026-11-07).
mod wrapped {
    use super::super::wrapped_call::{wrapped_call, WrappedCall};
    use crate::sdk_codegen::{ActionCommand, CommandDescriptor, CommandError, Ctx};
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Deserialize, ts_rs::TS, schemars::JsonSchema)]
    struct ShellP {
        cmd: String,
    }
    #[derive(Debug, Deserialize, ts_rs::TS, schemars::JsonSchema)]
    struct ListP {
        path: String,
    }
    #[derive(Debug, Deserialize, ts_rs::TS, schemars::JsonSchema)]
    struct ReadP {
        file_path: String,
        start_line: Option<u64>,
        end_line: Option<u64>,
    }
    #[derive(Debug, Deserialize, ts_rs::TS, schemars::JsonSchema)]
    struct ClaimRef(String);
    #[derive(Debug, Deserialize, ts_rs::TS, schemars::JsonSchema)]
    struct ClaimP {
        card_id: ClaimRef,
    }
    #[derive(Debug, Serialize, ts_rs::TS)]
    struct Done {}

    macro_rules! fixture {
        ($ty:ident, $name:literal, $aliases:expr, $params:ty) => {
            struct $ty;
            #[async_trait::async_trait]
            impl ActionCommand for $ty {
                const NAME: &'static str = $name;
                const ALIASES: &'static [&'static str] = $aliases;
                type Params = $params;
                type Output = Done;
                async fn run(&self, _ctx: &Ctx, _p: $params) -> Result<Done, CommandError> {
                    Ok(Done {})
                }
            }
        };
    }
    fixture!(Shell, "code/shell", &["bash"], ShellP);
    fixture!(List, "code/list", &["list_dir"], ListP);
    fixture!(Read, "code/read", &["read_file"], ReadP);
    fixture!(Claim, "work/claim", &[], ClaimP);

    /// A stand-in for the engine's name resolver over these four commands: canonical name, alias,
    /// or the charset-legal wire name (`work_claim`).
    fn resolve(name: &str) -> Option<&'static CommandDescriptor> {
        static ALL: std::sync::LazyLock<Vec<CommandDescriptor>> = std::sync::LazyLock::new(|| {
            vec![
                CommandDescriptor::of::<Shell>(),
                CommandDescriptor::of::<List>(),
                CommandDescriptor::of::<Read>(),
                CommandDescriptor::of::<Claim>(),
            ]
        });
        ALL.iter().find(|d| d.name == name || d.aliases.contains(&name) || d.name.replace('/', "_") == name)
    }

    // what this catches: `code/read(file_path: {"tool_call": {"name": "read_file", ...}})`, the
    // inner call naming THIS command by its alias, refused although its arguments were whole.
    #[test]
    fn a_wrapped_call_to_the_same_command_by_alias_runs_with_its_arguments() {
        let sent = json!({"file_path": {"tool_call": {"name": "read_file", "arguments": {"file_path": "src/lib.rs", "start_line": 1, "end_line": 100}}}});
        let Some(WrappedCall::Same { property, arguments }) = wrapped_call("code/read", &sent, resolve) else {
            panic!("the same command, by alias");
        };
        assert_eq!(property, "file_path");
        let read = resolve("code/read").expect("fixture"); // test: the fixture registry holds code/read
        assert_eq!(read.accept(arguments).expect("decodes").as_ref(), arguments); // test: a refusal here is the failure being caught
    }

    // what this catches: `work/claim(card_id: {"tool_call": {"name": "work/claim", ...}})`, named
    // canonically and by its wire name.
    #[test]
    fn a_wrapped_call_to_the_same_command_by_name_or_wire_name_runs() {
        for name in ["work/claim", "work_claim"] {
            let sent = json!({"card_id": {"tool_call": {"name": name, "arguments": {"card_id": "7af6c2d1"}}}});
            assert!(
                matches!(wrapped_call("work/claim", &sent, resolve), Some(WrappedCall::Same { property: "card_id", .. })),
                "{name}"
            );
        }
    }

    // what this catches: `bash(cmd: {"tool_call": {"name": "code/list", "arguments": {"path": "."}}})`:
    // her intent is the inner call, another command in her hands, so that call runs.
    #[test]
    fn a_wrapped_call_to_another_command_is_that_call() {
        let sent = json!({"cmd": {"tool_call": {"name": "code/list", "arguments": {"path": "."}}}});
        let Some(WrappedCall::Other { property, command, arguments }) = wrapped_call("code/shell", &sent, resolve) else {
            panic!("another command");
        };
        assert_eq!((property, command.name), ("cmd", "code/list"));
        assert!(command.accept(arguments).is_ok(), "the inner arguments are code/list's own");
    }

    // what this catches: an inner name in nobody's hands refused with the inner call as her next
    // call, never run under a guessed name.
    #[test]
    fn a_wrapped_call_naming_nothing_refuses_with_the_inner_call_next() {
        let sent = json!({"cmd": {"tool_call": {"name": "list_files", "arguments": {"path": "."}}}});
        let wrapped = wrapped_call("code/shell", &sent, resolve).expect("a wrapped call"); // test: no shape here is the failure being caught
        let WrappedCall::Unknown(unknown) = wrapped else { panic!("list_files names nothing") };
        assert_eq!((unknown.property, unknown.name), ("cmd", "list_files"));
        let said = unknown.refusal().to_string();
        assert!(said.contains(r#"list_files({"path":"."})"#), "{said}");
    }

    // what this catches: the shape read where the reading is a guess: beside another argument, a
    // tool_call with extra keys, or arguments that are not an object.
    #[test]
    fn a_wrapped_call_beside_other_arguments_is_not_read() {
        for sent in [
            json!({"cmd": {"tool_call": {"name": "code/list", "arguments": {"path": "."}}}, "wait_ms": 5}),
            json!({"cmd": {"tool_call": {"name": "code/list", "arguments": {"path": "."}, "id": "x"}}}),
            json!({"cmd": {"tool_call": {"name": "code/list", "arguments": "."}}}),
            json!({"cmd": {"tool_call": {"name": "code/list", "arguments": {}}, "other": 1}}),
        ] {
            assert!(wrapped_call("code/shell", &sent, resolve).is_none(), "{sent}");
        }
    }
}
