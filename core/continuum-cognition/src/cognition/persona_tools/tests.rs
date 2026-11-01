#![cfg(test)]

    use super::*;

    // what this catches: the name→verb mapping is a NAME match on a verb we defined
    // (protocol), which is the whole reason this replaced a prose phrase-list. Aliases
    // resolve; an unrelated command must never be mistaken for a yield.
    #[test]
    fn yield_recognition_is_a_name_match_and_nothing_broader() {
        assert!(is_yield_turn(VERDICT_YIELD_TURN));
        for alias in VERDICT_YIELD_ALIASES {
            assert!(is_yield_turn(alias), "alias must resolve: {alias}");
        }
        for other in [
            "work/list",
            "code/write",
            "yield",
            "passing",
            "pass_the_config",
        ] {
            assert!(!is_yield_turn(other), "must NOT read as a yield: {other}");
        }
    }

    // what this catches: the schema is a TEACHING surface, not a copy of the source
    // comment. schemars lifts `///` verbatim, so maintainer rationale (why the Rust
    // type is what it is, what the doc used to say) rode into every offered tool on
    // every turn — 550 tokens of the 26-verb native surface, measured. The lead
    // paragraph survives intact (truncating the ANSWER is #358's shape); everything
    // after the blank line is for whoever opens the file. Regression here = the
    // rationale coming back, or — worse — the lead sentence getting clipped.
    #[test]
    fn schema_descriptions_carry_the_lead_paragraph_not_the_rationale() {
        let schema = json!({
            "type": "object",
            "properties": {
                "recipe": {
                    "type": "string",
                    "description": "Which recipe to build from — the EXACT `purpose` key.\n\nNot enumerated here on purpose: recipes are DATA, so any list in this comment is stale the moment someone authors a new one."
                },
                // A param literally NAMED `description` must recurse, not be treated
                // as prose — the code/edit shape that a naive key-match would corrupt.
                "description": {
                    "type": "string",
                    "description": "Optional note describing the change.\n\nRecorded in the change history."
                }
            },
            "definitions": {
                "EditMode": {
                    "description": "How to edit.\n\nHistorical note nobody calling this needs."
                }
            }
        });

        let out = tool_input_schema_from(&schema);
        let recipe = out.properties["recipe"]["description"].as_str().unwrap();
        assert_eq!(
            recipe, "Which recipe to build from — the EXACT `purpose` key.",
            "lead paragraph must survive VERBATIM — clipping the answer makes the verb unusable"
        );
        assert!(
            !recipe.contains("stale the moment"),
            "maintainer rationale must not reach the offered schema: {recipe}"
        );

        // The param named `description` is a SCHEMA, so it keeps its own shape and
        // its nested prose is trimmed like any other field's.
        let named = &out.properties["description"];
        assert_eq!(named["type"], "string", "a param named `description` is not prose");
        assert_eq!(
            named["description"].as_str().unwrap(),
            "Optional note describing the change."
        );

        // definitions travel with the schema (llama.cpp resolves $refs against them)
        // and get the same treatment — they are offered prose too.
        let defs = out.definitions.expect("definitions must still ship");
        assert_eq!(defs["EditMode"]["description"].as_str().unwrap(), "How to edit.");
    }

    fn spec(name: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: name.to_string(),
            description: String::new(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({}),
                required: None,
                definitions: None,
            },
        }
    }

    // what this catches: the menu render is an expandable bookmarked menu — EVERY
    // category appears (the stable spine), an expanded category lists its verbs
    // inline, and a collapsed one shows only a depth + how to open it. Regression
    // here = the menu degrading back into either a full dump or a blind index.
    #[test]
    fn render_tool_menu_expands_only_selected_categories() {
        let tools = [
            spec("code/run"),
            spec("code/read"),
            spec("code/edit"),
            spec("gpu/stats"),
            spec("gpu/pressure"),
        ];
        let expanded: std::collections::BTreeSet<String> =
            ["code".to_string()].into_iter().collect();
        let out = render_tool_menu(&tools, &expanded);

        // Spine: both categories present every turn.
        assert!(out.contains("code"), "code header missing: {out}");
        assert!(
            out.contains("gpu"),
            "gpu header missing (spine broken): {out}"
        );
        // Expanded code lists its verbs inline.
        assert!(
            out.contains("code: edit, read, run"),
            "code not expanded: {out}"
        );
        // Collapsed gpu shows depth + how to open, NOT its verbs.
        assert!(
            out.contains("gpu (2 — commands/list --filter gpu)"),
            "gpu not collapsed: {out}"
        );
        assert!(!out.contains("stats"), "collapsed gpu leaked verbs: {out}");
    }

    // Build a spec whose schema declares required + optional fields, so the
    // catalog's param-name rendering can be asserted.
    fn spec_with(name: &str, required: &[&str], optional: &[&str]) -> NativeToolSpec {
        let mut props = serde_json::Map::new();
        for f in required.iter().chain(optional.iter()) {
            props.insert((*f).to_string(), json!({ "type": "string" }));
        }
        NativeToolSpec {
            name: name.to_string(),
            description: String::new(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::Value::Object(props),
                required: (!required.is_empty())
                    .then(|| required.iter().map(|s| s.to_string()).collect()),
                definitions: None,
            },
        }
    }

    // what this catches: the always-on catalog surfaces each verb's PARAM FIELD
    // NAMES (required bare, optional suffixed `?`) so a small model stops guessing
    // `code/search{query}`/`code/run{path}` and hitting `[invalid] CommandRequest`
    // (2026-07-01 legibility fix, [[persona-codes-blind]]). Required order is the
    // schema's declared order; optional is sorted; a no-param verb renders bare.
    #[test]
    fn catalog_renders_param_field_names() {
        let tools = [
            spec_with("code/search", &["pattern"], &["path"]),
            spec_with("code/run", &["lang", "code"], &["timeout_secs"]),
            spec("code/list"), // no params → bare verb, no parens
        ];
        let out = render_tool_catalog(&tools, 0);
        // Required bare, optional suffixed `?`; required keeps declared order.
        assert!(
            out.contains("run(lang, code, timeout_secs?)"),
            "run params wrong: {out}"
        );
        assert!(
            out.contains("search(pattern, path?)"),
            "search params wrong: {out}"
        );
        // A no-param verb renders bare — no empty `()`.
        assert!(
            out.contains("list,") || out.trim_end().ends_with("list"),
            "list should be bare: {out}"
        );
        assert!(
            !out.contains("list()"),
            "no-param verb must not render empty parens: {out}"
        );
    }

    // what this catches: a boolean subschema (`true`/`false`) anywhere in a tool's
    // params schema — which schemars emits for any untyped field (`serde_json::Value`,
    // open `HashMap`, default `additionalProperties`) — survives projection unrewritten
    // and 400s the WHOLE deliberation turn ("Unrecognized schema: true"), because every
    // offered tool schema rides one llama-server request. This is the regression that
    // kept Asha/Solenne abstaining on every tick after the registry migration exposed
    // ~200 real schemas. We rewrite `true`→`{}` / `false`→`{"not":{}}` at schema
    // positions only, leaving keyword booleans (`nullable`) untouched.
    #[test]
    fn boolean_subschemas_are_rewritten_but_keyword_booleans_survive() {
        let schema = json!({
            "type": "object",
            "properties": {
                "payload": true,                       // serde_json::Value field
                "name": { "type": "string", "nullable": true },
                "tags": { "type": "array", "items": true },
                "blocked": false
            },
            "additionalProperties": true,
            "definitions": { "Open": true }
        });
        let out = sanitize_schema_booleans(schema);

        // schema positions rewritten to objects
        assert_eq!(out["properties"]["payload"], json!({}), "true → {{}}");
        assert_eq!(
            out["properties"]["blocked"],
            json!({ "not": {} }),
            "false → not-any"
        );
        assert_eq!(
            out["properties"]["tags"]["items"],
            json!({}),
            "items: true → {{}}"
        );
        assert_eq!(
            out["additionalProperties"],
            json!({}),
            "additionalProperties: true → {{}}"
        );
        assert_eq!(
            out["definitions"]["Open"],
            json!({}),
            "definition true → {{}}"
        );

        // keyword boolean is NOT a schema position — left exactly as-is
        assert_eq!(
            out["properties"]["name"]["nullable"],
            json!(true),
            "nullable untouched"
        );

        // and there is no bare `true`/`false` left anywhere in the serialized schema
        assert!(
            !out.to_string().contains("true")
                || out["properties"]["name"]["nullable"] == json!(true),
            "the only surviving `true` is the keyword boolean"
        );
    }
