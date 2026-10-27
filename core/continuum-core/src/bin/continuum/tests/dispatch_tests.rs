#![cfg(test)]
use super::*;
use serde_json::json;

// Regression for card 25fadb8f: reboot --help stopped a live core. Every
// local verb must take the help return, including when --force is present;
// remote verbs must retain their schema-derived help path.
#[test]
fn lifecycle_help_precedes_actions_and_remote_help_stays_remote() {
    for command in [
        "start",
        "reboot",
        "restart",
        "boot",
        "stop",
        "desktop",
        "ui",
        "orphans",
        "deploy-verify",
        "verify",
        "checkpoint",
    ] {
        for flag in ["-h", "--help"] {
            assert!(super::local_help_requested(
                command,
                &["--force".into(), flag.into()]
            ));
        }
        assert!(!super::local_help_requested(command, &["--force".into()]));
    }
    for command in ["help", "-h", "--help"] {
        assert!(super::local_help_requested(command, &[]));
    }
    assert!(!super::local_help_requested(
        "serving/status",
        &["--help".into()]
    ));
}

// what this catches: the PROCEDURAL param adapter — one generic rule for all
// commands, no per-command switch. Covers the three forms + coercion +
// kebab/snake→camelCase + bare flags. This is the CLI edge of "meet humans/AIs
// in the middle at every interface" (Joel 2026-06-21).
#[test]
fn params_adapt_procedurally_from_args() {
    // No schema available → generic camelCase normalization (pre-schema behavior).
    let no_schema: &[String] = &[];

    // 1. nothing → empty object
    assert_eq!(params_from_args("ping", &[], no_schema).unwrap(), json!({}));

    // 2. positional JSON verbatim (the AI / tool-call path)
    assert_eq!(
        params_from_args("ping", &[r#"{"message":"hi"}"#.to_string()], no_schema).unwrap(),
        json!({ "message": "hi" })
    );

    // 3. --key value with coercion: string stays string, number→number,
    //    bool→bool; keys camelCased from kebab/snake.
    let p = params_from_args(
        "ping",
        &[
            "--message".into(),
            "hi".into(),
            "--round-trip-ms".into(),
            "5".into(),
            "--enabled".into(),
            "true".into(),
        ],
        no_schema,
    )
    .unwrap();
    assert_eq!(
        p,
        json!({ "message": "hi", "roundTripMs": 5, "enabled": true }),
        "coerced + camelCased automatically, one generic rule"
    );

    // bare flag (no value) → true
    assert_eq!(
        params_from_args("ping", &["--verbose".into()], no_schema).unwrap(),
        json!({ "verbose": true })
    );

    // `--key=value` form (muscle memory) — split on first `=`, NOT a junk key.
    assert_eq!(
        params_from_args("ping", &["--filter=data/".into()], no_schema).unwrap(),
        json!({ "filter": "data/" }),
        "--key=value splits correctly (regression: was {{\"filter=data/\": true}})"
    );
    assert_eq!(
        params_from_args("ping", &["--round-trip-ms=5".into()], no_schema).unwrap(),
        json!({ "roundTripMs": 5 }),
        "--key=value coerces + camelCases"
    );

    // a non-flag, non-JSON arg is a clear error (not silently swallowed)
    assert!(params_from_args("ping", &["oops".into()], no_schema).is_err());
}

// what this catches: snake_case Rust-native command fields (e.g. cognition/eval's
// `persona_id`) must be invokable by flag. continuum used to blanket-camelCase every key,
// so `--persona_id` became `personaId` and the server rejected it with
// `missing field persona_id`. With the command's schema known, any spelling of a
// schema field canonicalizes to the exact field name; flags NOT in the schema
// (base fields, schemaless commands) keep the legacy camelCase normalization.
// regression for the 2026-06-25 continuum flag bug.
#[test]
fn flags_canonicalize_to_schema_field_names() {
    let canonical = vec!["persona_id".to_string(), "eval_set".to_string()];
    // every separator/case spelling of a schema field → the exact field name
    for spelling in [
        "--persona_id",
        "--persona-id",
        "--personaId",
        "--PERSONA_ID",
    ] {
        let p = params_from_args("ping", &[spelling.into(), "abc".into()], &canonical).unwrap();
        assert_eq!(p, json!({ "persona_id": "abc" }), "{spelling} → persona_id");
    }
    // `--key=value` form canonicalizes too
    assert_eq!(
        params_from_args("ping", &["--eval-set=x.jsonl".into()], &canonical).unwrap(),
        json!({ "eval_set": "x.jsonl" })
    );
    // a flag NOT in a KNOWN schema is refused, not coerced (see the unknown-flag
    // test below). Identity fields like userId are injected by the connection —
    // no command's schema carries them (349/349 on 2026-09-20) — so there is no
    // base-field carve-out to preserve here.
    assert!(
        params_from_args("ping", &["--room-id".into(), "r1".into()], &canonical).is_err(),
        "non-schema flag against a known schema → refused"
    );
    // with no schema at all, everything is legacy camelCase
    assert_eq!(
        params_from_args("ping", &["--persona-id".into(), "abc".into()], &[]).unwrap(),
        json!({ "personaId": "abc" }),
        "no schema → legacy camelCase (pre-schema behavior preserved)"
    );
}

// what this catches: a typo'd flag being silently coerced into a junk param the
// command ignores, so the caller gets a successful-looking answer to a question
// they did not ask. Measured 2026-09-05: `continuum ping --nonsense-flag`
// returned a healthy pong, exit 0. The ladder is the point: refuse ONLY when the
// schema is known; with no schema we cannot prove a flag is unknown, so the
// camelCase guess must stand or every schemaless command becomes uncallable.
// regression for #3724.
#[test]
fn an_unknown_flag_is_refused_by_name_only_when_the_schema_is_known() {
    let schema = &["message".to_string()];
    let err = params_from_args("ping", &["--nonsense-flag".into()], schema)
        .expect_err("a flag the schema does not have must be refused, not coerced");
    assert!(err.contains("--nonsense-flag"), "names the offending flag: {err}");
    assert!(err.contains("`ping`"), "names the command: {err}");
    assert!(err.contains("--message"), "lists what IS accepted: {err}");
    // every arg shape refuses the same way: `--k=v` and `--k v`, not just bare
    assert!(params_from_args("ping", &["--nonsense=1".into()], schema).is_err());
    assert!(params_from_args("ping", &["--nonsense".into(), "1".into()], schema).is_err());

    // A real field still parses, and still canonicalizes across separators.
    assert_eq!(
        params_from_args("ping", &["--message".into(), "hi".into()], schema).unwrap(),
        json!({ "message": "hi" })
    );

    // NO schema → cannot prove unknown → the pre-schema behaviour is preserved.
    let no_schema: &[String] = &[];
    assert_eq!(
        params_from_args("ping", &["--nonsense-flag".into()], no_schema).unwrap(),
        json!({ "nonsenseFlag": true }),
        "without a schema an unrecognised flag must NOT be refused"
    );
}

#[test]
fn camel_case_normalizes_kebab_and_snake() {
    assert_eq!(to_camel_case("round-trip-ms"), "roundTripMs");
    assert_eq!(to_camel_case("round_trip_ms"), "roundTripMs");
    assert_eq!(to_camel_case("message"), "message");
    // round-trips with the display direction
    assert_eq!(camel_to_kebab("roundTripMs"), "round-trip-ms");
    assert_eq!(camel_to_kebab("message"), "message");
}

// what this catches: the CLI help adapter renders a command's manual in the
// bash paradigm from the SAME schema the AI adapter reads as a tool spec — flags
// (camelCase property → --kebab), types, descriptions, required markers. "The
// instructions manual matches the paradigm" (Joel 2026-06-21), single source.
#[test]
fn help_renders_schema_as_bash_flags() {
    let info = json!({
        "name": "ping",
        "description": "Health check.",
        "paramsSchema": {
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Echoed back." },
                "roundTripMs": { "type": "integer" }
            },
            "required": ["message"]
        }
    });
    let help = render_cli_help("ping", &info);
    assert!(help.contains("ping — Health check."), "header: {help}");
    assert!(help.contains("--message"), "flag from property: {help}");
    assert!(help.contains("Echoed back."), "prop description: {help}");
    assert!(help.contains("(required)"), "required marker: {help}");
    assert!(help.contains("--round-trip-ms"), "camel→kebab flag: {help}");
    assert!(help.contains("<integer>"), "type label: {help}");
}
