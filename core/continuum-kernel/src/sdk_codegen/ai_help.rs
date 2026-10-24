//! A command's manual in the AI paradigm, and the nearest names to a miss: pure functions of
//! a command's name, description and params JSON Schema, the same schema every interface
//! adapts from. Part of the command contract, so the kernel owns them; `commands/help`, the
//! socket's unknown-command reply and a mind's tool executor all render through here.

use std::collections::HashSet;

use serde_json::{json, Value};

/// How many commands a lookup names at once: the nearest names to a miss, and the matches a
/// `commands/help` query returns. Wide enough that the route survives the eight-way tie
/// `list_cards` produced (2026-09-05, see `did_you_mean`), narrow enough to read at a glance.
pub const SUGGESTION_WINDOW: usize = 6;

/// Closest authorized command names to a miss — same category prefix (before `/`),
/// or sharing a path segment. Cheap, dependency-free, and enough to unstick an agent
/// that guessed a plausible-but-wrong name (e.g. `commands/describe` → `commands/help`).
pub fn did_you_mean<'a>(query: &str, authorized: &[&'a str]) -> Vec<&'a str> {
    let q = query.to_lowercase();
    let q_prefix = q.split('/').next().unwrap_or(&q);
    let q_segs: HashSet<&str> = q.split('/').filter(|s| !s.is_empty()).collect();
    // Tokens across EVERY separator a caller might reach for, not just `/`. A mind
    // told its tools live behind `list_commands` forms `list_cards` by analogy — an
    // underscored bare name that shares no category prefix and no whole path segment
    // with anything, so the two tiers below discarded every candidate and the refusal
    // named nothing. Measured 2026-09-05: a citizen issued `list_cards` 31 times, the
    // loop-breaker told her so on 30 of them, and neither channel ever mentioned
    // `commands/list`. The perception side declines to say what to do next ON PURPOSE
    // (apply.rs: "It never says what to do instead (that would be steering)") — which
    // assumes THIS function carries the routing. Naming commands that exist is not
    // steering; it is the same class of fact as "you have called this 31 times".
    let q_tokens: HashSet<&str> = q
        .split(|c| c == '/' || c == '_' || c == '-')
        .filter(|s| !s.is_empty())
        .collect();
    let mut scored: Vec<(u8, &str)> = authorized
        .iter()
        .filter_map(|name| {
            let n = name.to_lowercase();
            let n_prefix = n.split('/').next().unwrap_or(&n);
            let shares_seg = n.split('/').any(|s| !s.is_empty() && q_segs.contains(s));
            let shares_token = n
                .split(|c| c == '/' || c == '_' || c == '-')
                .any(|s| !s.is_empty() && q_tokens.contains(s));
            let score = if n_prefix == q_prefix {
                2 // same category — strongest signal
            } else if shares_seg {
                1
            } else if shares_token {
                0 // a shared word — weakest, but never nothing
            } else {
                return None;
            };
            Some((score, *name))
        })
        .collect();
    // Rank: score, then DEPTH, then name.
    //
    // Alphabetical alone was the tie-break when #3769 landed, and on the very failure
    // that motivated it the right answers lost the race. Measured live 2026-09-05:
    //
    //   list_cards -> agent/list, ai/lora/list, ai/models/list, ai/providers/list,
    //                 benchmark/list, code/list
    //
    // Eight commands share the word `list`, all tie at score 0, and `take(6)` cut at
    // `code/` — dropping exactly `commands/list` and `work/list`, the two the citizen
    // wanted. Suggestions appeared where there had been silence, which was the fix,
    // and none of them was the route, which was the claim.
    //
    // Depth is the discriminator the data actually carries: a two-segment command is
    // a top-level verb, a three-segment one is a specialisation inside a family. When
    // nothing else separates candidates, the shallower command is the likelier answer
    // — `work/list` over `ai/providers/list`. It is only a tie-break, so the score
    // tiers above are untouched.
    //
    // Edit distance was the first thing I tried and it does not work here: every
    // candidate ENDS in `list`, so last-segment distance ties them all, and the
    // query's own last token is `cards`, which is near none of them. A discriminator
    // has to discriminate on the case that motivated it.
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(depth(a.1).cmp(&depth(b.1)))
            .then(a.1.cmp(b.1))
    });
    scored.into_iter().take(SUGGESTION_WINDOW).map(|(_, n)| n).collect()
}

/// Path segments in a command name — `work/list` is 2, `ai/providers/list` is 3.
/// Shallower is more central, and that is the only structural signal left once two
/// candidates share a word and nothing else.
fn depth(name: &str) -> usize {
    name.split('/').filter(|s| !s.is_empty()).count()
}

/// Render a command's manual in the AI paradigm: the canonical `{"tool_call": …}`
/// envelope (with placeholder values typed from the schema) + an argument list.
/// Pure function of (name, description, params JSON Schema) — same schema every
/// other interface adapts from.
pub fn render_ai_help(name: &str, description: &str, schema: &Value) -> String {
    let props = schema.get("properties").and_then(Value::as_object);
    let required: HashSet<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut example = serde_json::Map::new();
    let mut arg_lines: Vec<String> = Vec::new();
    if let Some(props) = props {
        for (key, spec) in props {
            let req = required.contains(key.as_str());
            let doc = spec
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("");
            // Resolve the param's real shape — a scalar `type`, OR an enum (`oneOf`/
            // `anyOf`, possibly behind a `$ref`/`allOf`) whose variants we EXPAND into
            // a hint + a concrete example. Without this, a complex param (e.g. an
            // `EditMode` enum) collapsed to a useless `"any"`, so a model literally
            // could not tell what to pass — the invisible-contract bug.
            let (ty, placeholder) = param_shape(spec, schema);
            // Only canonical required arguments belong in the example. Conditional
            // requirements must be represented by the owning command's typed schema;
            // optional compatibility fields are documented without becoming examples.
            if req {
                example.insert(key.clone(), placeholder);
            }
            arg_lines.push(format!(
                "- {key} ({ty}, {}){}",
                if req { "required" } else { "optional" },
                if doc.is_empty() {
                    String::new()
                } else {
                    format!(" — {doc}")
                },
            ));
        }
    }

    let envelope = json!({ "tool_call": { "name": name, "arguments": Value::Object(example) } });
    let envelope_str = serde_json::to_string_pretty(&envelope)
        .unwrap_or_else(|_| "{\"tool_call\": {\"name\": \"…\", \"arguments\": {}}}".to_string());

    let args_block = if arg_lines.is_empty() {
        "(no arguments)".to_string()
    } else {
        arg_lines.join("\n")
    };

    format!(
        "{name} — {description}\n\n\
         To call it, emit this SHAPE — every `<replace-with-…>` below is a blank you fill \
         in, not a value to send:\n{envelope_str}\n\n\
         Arguments:\n{args_block}"
    )
}

/// Resolve a schema node that may be a `$ref` (or a single-element `allOf` wrapping one —
/// schemars' usual shape for a named type) against the root schema's `definitions`/`$defs`.
/// Returns the node unchanged if it isn't a ref or the target is missing.
fn resolve_ref<'a>(spec: &'a Value, root: &'a Value) -> &'a Value {
    if let Some(items) = spec.get("allOf").and_then(Value::as_array) {
        if items.len() == 1 {
            return resolve_ref(&items[0], root);
        }
    }
    if let Some(r) = spec.get("$ref").and_then(Value::as_str) {
        let name = r.rsplit('/').next().unwrap_or("");
        for defs in ["definitions", "$defs"] {
            if let Some(d) = root.get(defs).and_then(|d| d.get(name)) {
                return d;
            }
        }
    }
    spec
}

/// Describe a parameter's shape for help: a human type hint + a concrete placeholder
/// example. Scalars → `("string", "<replace-with-string>")`. Externally-tagged enums (`oneOf`/`anyOf`,
/// possibly behind a `$ref`) → `("one of: A{fields} | B{fields}", <first-variant example>)`
/// so a model SEES the variants instead of a blind `"any"` (the invisible-contract bug that
/// left `code/edit`'s `edit_mode` uncallable). Graceful `any` fallback on any unknown shape.
fn param_shape(spec: &Value, root: &Value) -> (String, Value) {
    let spec = resolve_ref(spec, root);
    if let Some(value) = spec.get("const") {
        return (format!("constant {value}"), value.clone());
    }
    if let Some(types) = spec.get("type").and_then(Value::as_array) {
        let types: Vec<&str> = types.iter().filter_map(Value::as_str).collect();
        // JSON Schema permits a type array for nullable scalars. Describe every type,
        // but show a non-null value so the example demonstrates useful input.
        if let Some(ty) = types.iter().find(|ty| **ty != "null").or(types.first()) {
            let mut scalar = spec.clone();
            scalar["type"] = json!(ty);
            let (_, value) = param_shape(&scalar, root);
            return (types.join(" | "), value);
        }
    }
    if let Some(ty) = spec.get("type").and_then(Value::as_str) {
        if let Some(en) = spec.get("enum").and_then(Value::as_array) {
            let opts: Vec<String> = en
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            if !opts.is_empty() {
                return (
                    format!("one of: {}", opts.join(" | ")),
                    Value::String(opts[0].clone()),
                );
            }
        }
        // A placeholder must be VALID FOR ITS OWN TYPE. `<boolean>` is a string, so a
        // caller that copies the example verbatim — which is exactly what the manual
        // says to do, and exactly what a small model does — gets
        // `invalid type: string "<boolean>", expected a boolean` and cannot recover by
        // trying harder: the example it was handed is unsatisfiable. Observed three
        // times on 2026-09-05 (`code/edit <string>`, `code/edit … "<any>" expected a
        // boolean`). A typed placeholder still reads as fill-me-in, and a literal copy
        // now fails on MEANING (wrong path, missing field) instead of on SHAPE — an
        // error the caller can act on.
        //
        // THAT WAS NOT ENOUGH, and the eight days after it say so. `<string>` is what a
        // TYPE ANNOTATION looks like, so a caller reading "emit exactly this" over a
        // block containing `"card_id": "<string>"` sent exactly that — measured on
        // 2026-09-14, one citizen issuing `work/release({"card_id":"<string>",
        // "claim_id":"<string>"})` TWELVE times in a turn, and a second peer showing the
        // same signature in the event store since 09-05. She was not guessing; the
        // example told her to. Two words of prose ("fill in") cannot outvote a concrete,
        // authoritative, copyable block — the example always wins.
        //
        // So the placeholder must read as an IMPERATIVE rather than a type. It still
        // deserializes (the property won above), and a literal copy still fails on
        // meaning — but `<replace-with-string>` cannot be mistaken for a value the way
        // `<string>` can. See card 6c55f7a7.
        let placeholder = match ty {
            "boolean" => json!(true),
            "integer" | "number" => numeric_example(spec, ty == "integer"),
            "array" => json!([]),
            "object" => json!({}),
            "null" => Value::Null,
            // A string placeholder IS a valid string: it parses, then fails on content.
            _ => json!(format!("<replace-with-{ty}>")),
        };
        return (ty.to_string(), placeholder);
    }
    for tag in ["oneOf", "anyOf"] {
        if let Some(variants) = spec.get(tag).and_then(Value::as_array) {
            let mut names: Vec<String> = Vec::new();
            let mut example = json!("<any>");
            for (i, v) in variants.iter().enumerate() {
                let v = resolve_ref(v, root);
                let props = match v.get("properties").and_then(Value::as_object) {
                    Some(p) => p,
                    None => {
                        // a bare `const`/`enum` string variant (plain unit enum)
                        if let Some(c) = v.get("const").and_then(Value::as_str) {
                            names.push(format!("\"{c}\""));
                            if i == 0 {
                                example = json!(c);
                            }
                        }
                        continue;
                    }
                };
                // INTERNALLY-tagged (serde tag = "type"): a discriminator property whose
                // schema is a single-value const/enum string is the variant NAME; the other
                // properties are its fields, and the call is a FLAT object carrying the tag.
                let discr = props.iter().find_map(|(pk, pv)| {
                    let pv = resolve_ref(pv, root);
                    pv.get("const")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            pv.get("enum")
                                .and_then(Value::as_array)
                                .filter(|a| a.len() == 1)
                                .and_then(|a| a[0].as_str())
                        })
                        .map(|val| (pk.clone(), val.to_string()))
                });
                if let Some((tag_field, vname)) = discr {
                    let fields: Vec<String> =
                        props.keys().filter(|k| **k != tag_field).cloned().collect();
                    names.push(if fields.is_empty() {
                        vname.clone()
                    } else {
                        format!("{vname}{{{}}}", fields.join(", "))
                    });
                    if i == 0 {
                        let mut inner = serde_json::Map::new();
                        inner.insert(tag_field.clone(), json!(vname.clone()));
                        inner.extend(required_object_example(v, root));
                        example = Value::Object(inner);
                    }
                } else if let (1, Some((vname, vschema))) = (props.len(), props.iter().next()) {
                    // EXTERNALLY-tagged: the single property key IS the variant name.
                    let vs = resolve_ref(vschema, root);
                    let fields: Vec<String> = vs
                        .get("properties")
                        .and_then(Value::as_object)
                        .map(|o| o.keys().cloned().collect())
                        .unwrap_or_default();
                    names.push(if fields.is_empty() {
                        vname.clone()
                    } else {
                        format!("{vname}{{{}}}", fields.join(", "))
                    });
                    if i == 0 {
                        let inner = required_object_example(vs, root);
                        example = json!({ vname.clone(): Value::Object(inner) });
                    }
                }
            }
            if !names.is_empty() {
                return (format!("one of: {}", names.join(" | ")), example);
            }
        }
    }
    ("any".to_string(), json!("<any>"))
}

/// Required fields retain their actual JSON types, including inside tagged variants.
fn required_object_example(spec: &Value, root: &Value) -> serde_json::Map<String, Value> {
    let mut result = serde_json::Map::new();
    if let Some(required) = spec.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            if let Some(field) = spec.get("properties").and_then(|p| p.get(name)) {
                let (_, value) = param_shape(field, root);
                result.insert(name.to_owned(), value);
            }
        }
    }
    result
}

fn numeric_example(spec: &Value, integer: bool) -> Value {
    let minimum = spec.get("minimum").and_then(Value::as_f64);
    let exclusive = spec.get("exclusiveMinimum").and_then(Value::as_f64);
    let mut value = minimum.unwrap_or(0.0);
    if integer {
        value = value.ceil();
    }
    if let Some(bound) = exclusive {
        let above = if integer { bound.floor() + 1.0 } else { bound.next_up() };
        value = value.max(above);
    }
    if integer && value >= 0.0 {
        json!(value as u64)
    } else if integer {
        json!(value as i64)
    } else {
        json!(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: an externally-tagged enum param (code/edit's EditMode) is EXPANDED
    // into its variants + a concrete example instead of collapsing to a useless "any" — the
    // invisible-contract bug I hit firsthand taking the SWE-bench test through continuum (a model
    // literally could not tell what edit_mode wanted). Fixture mirrors schemars output
    // ($ref → definitions with a oneOf of {Variant:{fields}}).
    #[test]
    fn help_expands_enum_param_instead_of_any() {
        // EditMode is #[serde(tag = "type", rename_all = "snake_case")] — INTERNALLY tagged.
        // schemars renders each variant as an object with a single-enum `type` discriminator
        // plus the variant's fields; the real call is a FLAT `{"type":"search_replace", ...}`.
        let schema = json!({
            "type": "object",
            "required": ["file_path", "edit_mode"],
            "properties": {
                "file_path": {"type": "string", "description": "path"},
                "edit_mode": {"$ref": "#/definitions/EditMode"}
            },
            "definitions": { "EditMode": {"oneOf": [
                {"type":"object","required":["type","search","replace"],"properties":{"type":{"type":"string","enum":["search_replace"]},"search":{"type":"string"},"replace":{"type":"string"},"all":{"type":"boolean"}}},
                {"type":"object","required":["type","content"],"properties":{"type":{"type":"string","enum":["append"]},"content":{"type":"string"}}}
            ]}}
        });
        let out = render_ai_help("code/edit", "edit a file", &schema);
        assert!(
            out.contains("search_replace{"),
            "variant NAME (not field) + fields shown: {out}"
        );
        assert!(
            out.contains("search") && out.contains("replace"),
            "variant fields shown: {out}"
        );
        assert!(out.contains("append"), "second variant shown: {out}");
        assert!(
            !out.contains("edit_mode (any"),
            "no longer collapses to any: {out}"
        );
        // the example is a FLAT object carrying the discriminator — the shape that actually works
        assert!(
            out.contains("\"type\"") && out.contains("\"search_replace\""),
            "flat tagged example: {out}"
        );
    }

    // what this catches: a plain scalar param still renders as before (no regression).
    #[test]
    fn scalar_param_unchanged() {
        let schema = json!({"type":"object","required":["p"],"properties":{"p":{"type":"string"}}});
        let out = render_ai_help("x", "y", &schema);
        assert!(out.contains("p (string, required)"), "{out}");
    }

    #[test]
    fn nullable_scalar_and_tagged_fields_keep_their_json_types() {
        let schema = json!({"type": ["integer", "null"]});
        assert_eq!(param_shape(&schema, &schema), ("integer | null".into(), json!(0)));
        let schema = json!({"oneOf": [{"type":"object",
            "required":["type", "count", "enabled"], "properties": {
                "type":{"const":"example"}, "count":{"type":"integer"},
                "enabled":{"type":"boolean"}, "optional":{"type":"string"}
            }}]});
        let (_, example) = param_shape(&schema, &schema);
        assert_eq!(example, json!({"type":"example", "count":0, "enabled":true}));
        assert_eq!(numeric_example(&json!({"minimum":1}), true), json!(1));
        assert_eq!(numeric_example(&json!({"exclusiveMinimum":1}), true), json!(2));
        assert!(numeric_example(&json!({"exclusiveMinimum":0.5}), false).as_f64().unwrap() > 0.5);
    }

    // what this catches: regression for card 6c55f7a7 — a string placeholder that reads
    // as a TYPE ANNOTATION rather than as a blank. `<string>` is exactly what a type
    // looks like, so citizens sent it as a value: measured 2026-09-14, one issuing
    // `work/release({"card_id":"<string>","claim_id":"<string>"})` twelve times in a
    // single turn, and a second peer showing the same signature since 09-05. The manual
    // must not contain a token a caller can mistake for a value, and must not instruct
    // literal emission of the block that holds it.
    #[test]
    fn the_example_never_offers_a_token_that_reads_as_a_value() {
        let schema = json!({
            "type": "object",
            "required": ["card_id"],
            "properties": {"card_id": {"type": "string"}}
        });
        let out = render_ai_help("work/release", "release a claim", &schema);
        assert!(
            !out.contains("\"<string>\""),
            "a bare `<string>` reads as a type annotation and gets sent as a value: {out}"
        );
        assert!(
            out.contains("<replace-with-string>"),
            "the blank must read as an imperative: {out}"
        );
        assert!(
            !out.contains("emit exactly this"),
            "instructing literal emission over a block of blanks is the defect — the \
             concrete example outvotes the prose every time: {out}"
        );
    }

    // what this catches: a placeholder that is invalid for its OWN type, so a caller
    // copying the example verbatim — which the manual instructs — cannot possibly
    // succeed. Observed 2026-09-05: `code/edit` refused with
    // `invalid type: string "<any>", expected a boolean`, the citizen having pasted the
    // placeholder it was shown. The example must at least DESERIALIZE; being wrong
    // about the value is recoverable, being wrong about the shape is not.
    #[test]
    fn every_placeholder_deserializes_as_its_own_type() {
        let schema = json!({
            "type": "object",
            "properties": {
                "flag":  {"type": "boolean"},
                "count": {"type": "integer"},
                "items": {"type": "array"},
                "opts":  {"type": "object"},
                "name":  {"type": "string"},
            }
        });
        for (field, ty) in [
            ("flag", "boolean"),
            ("count", "integer"),
            ("items", "array"),
            ("opts", "object"),
            ("name", "string"),
        ] {
            let spec = &schema["properties"][field];
            let (_, placeholder) = param_shape(spec, &schema);
            let ok = match ty {
                "boolean" => placeholder.is_boolean(),
                "integer" => placeholder.is_number(),
                "array" => placeholder.is_array(),
                "object" => placeholder.is_object(),
                _ => placeholder.is_string(),
            };
            assert!(ok, "{field} ({ty}) placeholder is not a {ty}: {placeholder}");
        }
    }

    // what this catches: the example envelope carrying OPTIONAL arguments, which a
    // caller then copies wholesale. Measured 2026-09-05: a citizen emitted all ten of
    // `code/edit`'s arguments back as a tool call — nine optional, every one a
    // placeholder — because that is what the manual showed her. The example must be
    // the minimal valid call; the optional arguments stay documented in the list.
    #[test]
    fn the_example_carries_required_arguments_only() {
        let schema = json!({
            "type": "object",
            "required": ["file_path"],
            "properties": {
                "file_path": {"type": "string"},
                "replace":   {"type": "boolean"},
                "line":      {"type": "integer"},
            }
        });
        let out = render_ai_help("code/edit", "edit a file", &schema);
        let envelope: Value = {
            let start = out.find('{').expect("an envelope is rendered");
            let end = out.rfind('}').expect("an envelope is rendered");
            serde_json::from_str(&out[start..=end]).expect("the rendered envelope is valid JSON")
        };
        let args = &envelope["tool_call"]["arguments"];
        assert!(args.get("file_path").is_some(), "the required arg is in the example: {args}");
        assert!(
            args.get("replace").is_none() && args.get("line").is_none(),
            "optional args must NOT be in the copyable example: {args}"
        );
        // …but they are still DOCUMENTED, or the caller cannot discover them.
        assert!(out.contains("replace (boolean, optional)"), "{out}");
        assert!(out.contains("line (integer, optional)"), "{out}");
    }
}
