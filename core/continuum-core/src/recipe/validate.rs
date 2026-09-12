//! Pipeline validation — a recipe's steps checked against the COMMAND REGISTRY'S OWN
//! SCHEMAS before anything runs, the way a request is checked against an API spec.
//!
//! Every command already declares a JSON Schema for its params
//! (`CommandDescriptor::params_schema`). Until this module, an authored pipeline
//! learned it named a verb that does not exist, or a param the verb never declared,
//! at RUN time — after the room was born. Now `activity/recipes` reports every issue
//! per recipe, and `activity/spawn` refuses a recipe that has any, naming each.
//!
//! DEFINITIVE, not key-matching: a literal value is checked against the declared
//! `type` and `enum` (through `$ref` definitions); every `$reference` is RESOLVED —
//! `$args.x` to a declared recipe param whose default's type must fit the command's
//! param, `$room.*` to the fields the birth path seeds, `$item`/`$index` only inside an
//! `each` step, and any other root to an `outputTo` bound by an EARLIER step. What is
//! not yet definitive is the SHAPE of a prior step's output (`$imported.cards`): command
//! outputs carry a TS type, not a JSON Schema — the next step is `Output: JsonSchema`.
//!
//! Pure over a lookup, so it is testable without the registry; the one production
//! lookup ([`registry_lookup`]) is built once per process. Cost: once per spawn and
//! once per catalogue read — never on a turn.

use super::types::RecipeStep;
use crate::experience::recipe::ExperienceRecipe;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// One thing wrong with one step. Typed and positional, so a renderer can point at
/// the step and an author can fix it without guessing.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/experience/PipelineIssue.ts")]
#[serde(rename_all = "camelCase")]
pub struct PipelineIssue {
    /// Zero-based index of the step in `pipeline[]`.
    pub step: u32,
    pub command: String,
    #[serde(flatten)]
    pub kind: IssueKind,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/experience/IssueKind.ts")]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IssueKind {
    /// No command by this name (or alias) is registered on this node.
    UnknownCommand,
    /// The step passes a param the command's schema does not declare.
    UnknownParam { name: String, declared: Vec<String> },
    /// The command's schema requires a param the step does not pass.
    MissingRequired { name: String },
    /// A literal value's JSON type is not one the schema allows for this param.
    TypeMismatch { name: String, expected: Vec<String>, got: String },
    /// A literal value is not one of the schema's enumerated values.
    NotInEnum { name: String, allowed: Vec<String>, got: String },
    /// `params` must be an object (or absent), never a scalar or an array.
    ParamsNotAnObject,
    /// `each` must be a `$…` reference to a binding, never a literal.
    EachNotAReference { value: String },
    /// `$args.<name>` names a param the recipe does not declare.
    UndeclaredArg { name: String, declared: Vec<String> },
    /// `$room.<field>` names a field the birth path does not seed.
    UnknownRoomField { field: String, seeded: Vec<String> },
    /// `$item` / `$index` used by a step that has no `each`.
    ItemOutsideEach { reference: String },
    /// `$<root>` names nothing: not args, room, item, or an earlier step's `outputTo`.
    UnboundReference { reference: String, bound_so_far: Vec<String> },
}

impl std::fmt::Display for PipelineIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "step {} ({}): ", self.step, self.command)?;
        match &self.kind {
            IssueKind::UnknownCommand => write!(f, "no such command on this node (commands/list)"),
            IssueKind::UnknownParam { name, declared } => {
                write!(f, "unknown param `{name}` — declared: [{}]", declared.join(", "))
            }
            IssueKind::MissingRequired { name } => write!(f, "required param `{name}` is missing"),
            IssueKind::TypeMismatch { name, expected, got } => {
                write!(f, "param `{name}` is {got}, schema allows [{}]", expected.join("|"))
            }
            IssueKind::NotInEnum { name, allowed, got } => {
                write!(f, "param `{name}` = {got:?} is not one of [{}]", allowed.join(", "))
            }
            IssueKind::ParamsNotAnObject => write!(f, "params must be an object"),
            IssueKind::EachNotAReference { value } => {
                write!(f, "`each` must be a $reference to an array binding, got {value:?}")
            }
            IssueKind::UndeclaredArg { name, declared } => {
                write!(f, "$args.{name} is not a declared param — declared: [{}]", declared.join(", "))
            }
            IssueKind::UnknownRoomField { field, seeded } => {
                write!(f, "$room.{field} is not seeded — the room offers: [{}]", seeded.join(", "))
            }
            IssueKind::ItemOutsideEach { reference } => {
                write!(f, "{reference} is only bound inside a step with `each`")
            }
            IssueKind::UnboundReference { reference, bound_so_far } => {
                write!(f, "{reference} is bound by nothing before this step — bound so far: [{}]", bound_so_far.join(", "))
            }
        }
    }
}

/// Fields every command accepts on the WIRE ENVELOPE, outside its own params
/// schema (`runtime::command_envelope::CommandRequest`, flattened). A step may pass
/// them to any command. Derived from that struct's fields on 2026-09-11; the
/// envelope-with-schema export (card 0674be4c) is what retires this list.
pub const ENVELOPE_FIELDS: &[&str] = &["actorKind", "contextId", "handle", "requestId", "sessionId", "userId"];

/// What the birth path seeds under `$room` (`modules::activity::spawn_activity_room`).
/// ONE list, asserted against the seeding site by test.
pub const ROOM_FIELDS: &[&str] = &["id", "name", "recipe"];

/// The command registry, projected to what validation needs: name (and every
/// alias) → the params schema. Built once.
pub fn registry_lookup() -> &'static BTreeMap<String, Value> {
    static LOOKUP: std::sync::OnceLock<BTreeMap<String, Value>> = std::sync::OnceLock::new();
    LOOKUP.get_or_init(|| {
        let mut m = BTreeMap::new();
        for d in crate::sdk_codegen::command_registry() {
            m.insert(d.name.to_string(), d.params_schema.clone());
            for alias in d.aliases {
                m.entry(alias.to_string()).or_insert_with(|| d.params_schema.clone());
            }
        }
        m
    })
}

/// The JSON type name of a value, in JSON-Schema vocabulary.
fn json_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => if n.is_i64() || n.is_u64() { "integer" } else { "number" },
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Follow a `$ref` to `#/definitions/<name>` (schemars 0.8) — one level is all the
/// generated schemas use for enums and nested params.
fn deref<'a>(prop: &'a Value, root: &'a Value) -> &'a Value {
    prop.get("$ref")
        .and_then(Value::as_str)
        .and_then(|r| r.strip_prefix("#/definitions/"))
        .and_then(|name| root.get("definitions").and_then(|d| d.get(name)))
        .unwrap_or(prop)
}

/// The alternatives a schema is a union of — schemars emits `anyOf` for `Option<T>`
/// and `oneOf` for an enum whose variants carry doc comments (one single-value
/// `enum` per variant, no top-level `type`). Both are unions; walk both.
fn alternatives(p: &Value) -> Option<&Vec<Value>> {
    p.get("anyOf").or_else(|| p.get("oneOf")).and_then(Value::as_array)
}

/// The JSON types a property schema admits. `None` = untyped (anything goes).
fn allowed_types(prop: &Value, root: &Value) -> Option<Vec<String>> {
    let p = deref(prop, root);
    match p.get("type") {
        Some(Value::String(s)) => Some(vec![s.clone()]),
        Some(Value::Array(items)) => Some(items.iter().filter_map(Value::as_str).map(str::to_string).collect()),
        _ => {
            let mut out: Vec<String> = Vec::new();
            for alt in alternatives(p)? {
                for ty in allowed_types(alt, root).unwrap_or_default() {
                    if !out.contains(&ty) { out.push(ty); }
                }
            }
            (!out.is_empty()).then_some(out)
        }
    }
}

/// The enumerated values a property admits, if it is an enum — through `$ref`, and
/// through `anyOf`/`oneOf` (an optional enum; a documented enum's per-variant lists,
/// which are unioned).
fn allowed_enum(prop: &Value, root: &Value) -> Option<Vec<String>> {
    let p = deref(prop, root);
    if let Some(e) = p.get("enum").and_then(Value::as_array) {
        return Some(e.iter().filter_map(Value::as_str).map(str::to_string).collect());
    }
    let mut out: Vec<String> = Vec::new();
    for alt in alternatives(p)? {
        out.extend(allowed_enum(alt, root).unwrap_or_default());
    }
    (!out.is_empty()).then_some(out)
}

/// A literal value against a property schema: type, then enum.
fn check_literal(name: &str, value: &Value, prop: &Value, root: &Value) -> Option<IssueKind> {
    let got = json_type(value);
    if let Some(expected) = allowed_types(prop, root) {
        let fits = expected.iter().any(|t| t == got || (t == "number" && got == "integer"));
        if !fits {
            return Some(IssueKind::TypeMismatch { name: name.to_string(), expected, got: got.to_string() });
        }
    }
    if let (Some(allowed), Value::String(s)) = (allowed_enum(prop, root), value) {
        if !allowed.iter().any(|a| a == s) {
            return Some(IssueKind::NotInEnum { name: name.to_string(), allowed, got: s.clone() });
        }
    }
    None
}

/// Every `$reference` inside a value: whole-value `$a.b` and embedded `${a.b}`.
fn references(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            if let Some(path) = s.strip_prefix('$') {
                if !path.is_empty() && !path.contains('{') && !s.contains(' ') {
                    out.push(path.to_string());
                    return;
                }
            }
            let mut rest = s.as_str();
            while let Some(start) = rest.find("${") {
                let after = &rest[start + 2..];
                let Some(end) = after.find('}') else { break };
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
        }
        Value::Array(items) => items.iter().for_each(|v| references(v, out)),
        Value::Object(map) => map.values().for_each(|v| references(v, out)),
        _ => {}
    }
}

/// A single `$reference` (path without the `$`) resolved against what the step can
/// see: the recipe's declared params, the seeded room, `each`'s item, and every
/// `outputTo` bound by an earlier step. `arg_type_for` lets a typed `$args.x`
/// be checked against the command's param schema by its default's JSON type.
fn check_reference(
    path: &str,
    recipe: &ExperienceRecipe,
    in_each: bool,
    bound: &[String],
) -> Option<IssueKind> {
    let (root, rest) = path.split_once('.').unwrap_or((path, ""));
    match root {
        "args" => {
            let name = rest.split('.').next().unwrap_or("");
            if name.is_empty() || !recipe.params.contains_key(name) {
                return Some(IssueKind::UndeclaredArg {
                    name: name.to_string(),
                    declared: recipe.params.keys().cloned().collect(),
                });
            }
            None
        }
        "room" => {
            let field = rest.split('.').next().unwrap_or("");
            if !ROOM_FIELDS.contains(&field) {
                return Some(IssueKind::UnknownRoomField {
                    field: field.to_string(),
                    seeded: ROOM_FIELDS.iter().map(|s| s.to_string()).collect(),
                });
            }
            None
        }
        "item" | "index" => (!in_each).then(|| IssueKind::ItemOutsideEach { reference: format!("${path}") }),
        other => (!bound.iter().any(|b| b == other)).then(|| IssueKind::UnboundReference {
            reference: format!("${path}"),
            bound_so_far: bound.to_vec(),
        }),
    }
}

/// Every issue in the recipe's pipeline, in step order. Empty = well-formed against
/// this node's registry. `lookup(command)` returns the command's params schema
/// (`Null` when the command declares none — then only its existence and the shape of
/// `params` are checked, never its keys).
pub fn pipeline_issues(recipe: &ExperienceRecipe, lookup: impl Fn(&str) -> Option<Value>) -> Vec<PipelineIssue> {
    let mut out = Vec::new();
    let mut bound: Vec<String> = Vec::new();
    for (i, step) in recipe.pipeline.iter().enumerate() {
        let issue = |kind: IssueKind| PipelineIssue { step: i as u32, command: step.command.clone(), kind };
        let Some(schema) = lookup(&step.command) else {
            out.push(issue(IssueKind::UnknownCommand));
            if let Some(o) = &step.output_to { bound.push(o.clone()); }
            continue;
        };
        let in_each = step.each.is_some();
        if let Some(each) = &step.each {
            match each.strip_prefix('$') {
                Some(path) if !path.is_empty() => {
                    if let Some(k) = check_reference(path, recipe, false, &bound) { out.push(issue(k)); }
                }
                _ => out.push(issue(IssueKind::EachNotAReference { value: each.clone() })),
            }
        }
        let given: BTreeMap<&str, &Value> = match &step.params {
            Value::Null => BTreeMap::new(),
            Value::Object(map) => map.iter().map(|(k, v)| (k.as_str(), v)).collect(),
            _ => {
                out.push(issue(IssueKind::ParamsNotAnObject));
                continue;
            }
        };
        // Every reference in every value resolves, whatever the command's schema says.
        let mut refs = Vec::new();
        references(&step.params, &mut refs);
        for r in refs {
            if let Some(k) = check_reference(&r, recipe, in_each, &bound) { out.push(issue(k)); }
        }
        if let Some(props) = schema.get("properties").and_then(Value::as_object) {
            let declared: Vec<String> = props.keys().cloned().collect();
            let envelope: HashSet<&str> = ENVELOPE_FIELDS.iter().copied().collect();
            for (name, value) in &given {
                match props.get(*name) {
                    Some(prop) => {
                        // A `$args.x` literal is typed by its DECLARED DEFAULT; other
                        // references are bound at run time and cannot be typed here yet.
                        let literal: Option<&Value> = match value {
                            Value::String(s) if s.starts_with('$') => s
                                .strip_prefix("$args.")
                                .and_then(|n| recipe.params.get(n))
                                .map(|d| &d.default),
                            Value::String(s) if s.contains("${") => None,
                            v => Some(v),
                        };
                        if let Some(lit) = literal {
                            if let Some(k) = check_literal(name, lit, prop, &schema) { out.push(issue(k)); }
                        }
                    }
                    None if envelope.contains(name) => {}
                    None => out.push(issue(IssueKind::UnknownParam { name: name.to_string(), declared: declared.clone() })),
                }
            }
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for r in required.iter().filter_map(Value::as_str) {
                    if !given.contains_key(r) {
                        out.push(issue(IssueKind::MissingRequired { name: r.to_string() }));
                    }
                }
            }
        }
        if let Some(o) = &step.output_to {
            bound.push(o.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(json: &str) -> ExperienceRecipe {
        ExperienceRecipe::from_json(json).expect("fixture recipe parses")
    }

    fn lookup(name: &str) -> Option<Value> {
        match name {
            "work/create" => Some(serde_json::json!({
                "type": "object",
                "required": ["repo", "title"],
                "properties": {
                    "repo": { "type": "string" }, "title": { "type": "string" },
                    "body": { "type": ["string", "null"] }, "room": {},
                    "priority": { "anyOf": [ { "$ref": "#/definitions/Priority" }, { "type": "null" } ] }
                },
                "definitions": { "Priority": { "type": "string", "enum": ["p0", "p1", "p2", "p3"] } }
            })),
            "benchmark/import" => Some(serde_json::json!({
                "type": "object", "required": ["suite"],
                "properties": { "suite": { "type": "string" }, "sample": { "type": ["integer", "null"] } }
            })),
            "commands/list" => Some(Value::Null),
            _ => None,
        }
    }

    // what this catches: the whole point. A typo'd verb, an undeclared param, a
    // missing required one, a literal of the wrong type, a value outside an enum —
    // each named BEFORE a room is born, with the step and the exact defect. Envelope
    // fields and well-formed references must not be flagged.
    #[test]
    fn literals_are_typed_and_every_defect_names_its_step() {
        let r = recipe(r#"{
            "purpose": "t", "regions": [],
            "params": { "repo": { "default": "acme/x" }, "n": { "default": 3 } },
            "pipeline": [
                { "command": "work/creat" },
                { "command": "work/create", "params": { "repo": "$args.repo", "titel": "x", "sessionId": "s" } },
                { "command": "work/create", "params": { "repo": "r", "title": 7, "priority": "urgent" } },
                { "command": "benchmark/import", "params": { "suite": "$args.repo", "sample": "$args.n" } },
                { "command": "commands/list", "params": { "anything": true } }
            ]
        }"#);
        let issues = pipeline_issues(&r, lookup);
        let rendered: Vec<String> = issues.iter().map(ToString::to_string).collect();
        // Step order is fixed; within a step, params are walked in key order — assert
        // by (step, defect), never by a position that a key rename would shuffle.
        let has = |step: u32, pred: &dyn Fn(&IssueKind) -> bool| issues.iter().any(|i| i.step == step && pred(&i.kind));
        assert!(has(0, &|k| *k == IssueKind::UnknownCommand), "{rendered:?}");
        assert!(has(1, &|k| matches!(k, IssueKind::UnknownParam { name, .. } if name == "titel")), "{rendered:?}");
        assert!(has(1, &|k| matches!(k, IssueKind::MissingRequired { name } if name == "title")), "{rendered:?}");
        assert!(has(2, &|k| matches!(k, IssueKind::TypeMismatch { name, got, .. } if name == "title" && got == "integer")), "{rendered:?}");
        assert!(has(2, &|k| matches!(k, IssueKind::NotInEnum { name, got, .. } if name == "priority" && got == "urgent")), "{rendered:?}");
        assert_eq!(issues.len(), 5, "envelope fields, typed $args and untyped commands are never flagged: {rendered:?}");
    }

    // what this catches: references are RESOLVED, not pattern-matched. `$args.x` must
    // be declared; `$room.x` must be seeded; `$item` needs `each`; any other root must
    // be an earlier step's outputTo — and a typed `$args.x` is checked by its default.
    #[test]
    fn references_resolve_or_are_named() {
        let r = recipe(r#"{
            "purpose": "t", "regions": [],
            "params": { "suite": { "default": "swe" }, "n": { "default": "three" } },
            "pipeline": [
                { "command": "benchmark/import", "params": { "suite": "$args.suit", "sample": "$args.n" }, "outputTo": "imported" },
                { "command": "work/create", "params": { "repo": "$room.owner", "title": "$item.title" } },
                { "command": "work/create", "each": "$imported.cards", "params": { "repo": "$room.id", "title": "$item.title" }, "outputTo": "cards" },
                { "command": "work/create", "params": { "repo": "${cards.0.card_id}", "title": "$later" } }
            ]
        }"#);
        let issues = pipeline_issues(&r, lookup);
        let rendered: Vec<String> = issues.iter().map(ToString::to_string).collect();
        assert!(matches!(&issues[0].kind, IssueKind::UndeclaredArg { name, .. } if name == "suit"), "{rendered:?}");
        assert!(matches!(&issues[1].kind, IssueKind::TypeMismatch { name, got, .. } if name == "sample" && got == "string"), "{rendered:?}");
        assert!(matches!(&issues[2].kind, IssueKind::UnknownRoomField { field, .. } if field == "owner"), "{rendered:?}");
        assert!(matches!(&issues[3].kind, IssueKind::ItemOutsideEach { .. }), "{rendered:?}");
        assert!(matches!(&issues[4].kind, IssueKind::UnboundReference { reference, .. } if reference == "$later"), "{rendered:?}");
        assert_eq!(issues.len(), 5, "{rendered:?}");
    }

    // what this catches: `each` over a literal would fan out over nothing an author
    // could see; params that are not an object cannot be interpolated by key.
    #[test]
    fn each_must_reference_and_params_must_be_an_object() {
        let r = recipe(r#"{
            "purpose": "t", "regions": [],
            "pipeline": [
                { "command": "work/create", "each": "cards", "params": { "repo": "r", "title": "t" } },
                { "command": "work/create", "params": "not an object" }
            ]
        }"#);
        let issues = pipeline_issues(&r, lookup);
        assert!(matches!(&issues[0].kind, IssueKind::EachNotAReference { value } if value == "cards"));
        assert_eq!(issues[1].kind, IssueKind::ParamsNotAnObject);
    }

    // what this catches: the shipped authored round must be well-formed against the
    // REAL registry on this build — every verb exists, every param is declared and
    // typed, every reference resolves. A renamed verb or a dropped param names the
    // step here, before a deploy. (This test found round-track's skipped schema
    // fields on its first run.)
    #[test]
    fn the_shipped_authored_round_is_well_formed_against_the_real_registry() {
        let r = recipe(include_str!("../experience/recipes/benchmark-round.json"));
        let issues = pipeline_issues(&r, |name| registry_lookup().get(name).cloned());
        let rendered: Vec<String> = issues.iter().map(ToString::to_string).collect();
        assert!(issues.is_empty(), "benchmark/round has pipeline issues: {rendered:#?}");
    }

    // what this catches: ROOM_FIELDS is asserted against the seeding site — if the
    // birth path seeds a new `$room` field, validation must learn it in the same PR.
    #[test]
    fn room_fields_match_what_the_birth_path_seeds() {
        let src = include_str!("../modules/activity.rs");
        let seed_site = src.find("\"room\".to_string(),").expect("the $room seed site exists");
        let window = &src[seed_site..seed_site + 400];
        for f in ROOM_FIELDS {
            assert!(window.contains(&format!("\"{f}\"")), "activity.rs seeds $room.{f}");
        }
    }
}
