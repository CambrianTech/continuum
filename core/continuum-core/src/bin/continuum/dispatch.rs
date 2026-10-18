//! `continuum <command>`: the CLI's edge of the uniform command surface — adapt bash
//! flags into the command's params (one procedural rule for every command, canonicalized
//! against the live `commands/list` schema), dispatch over the socket, and render a
//! command's manual in the bash paradigm from that same schema.

use continuum_client::ClientError;
use serde_json::Value;

use super::CliError;

use super::client::{connection, ensure_core_running};

/// `continuum <command> --help` — the CLI adapter for the command's manual: query the
/// live registry (`commands/list`) for the command's description + params schema,
/// then render it as bash usage. Same single source the AI tool adapter reads;
/// only the rendering differs by paradigm ("the manual matches the paradigm").
pub(super) async fn help_for(command: &str) -> Result<(), CliError> {
    ensure_core_running(command).await?; // the manual comes from the live registry
    let list = connection()
        .commands()
        .execute_value("commands/list", serde_json::json!({ "filter": command }))
        .await
        .map_err(|e| format!("commands/list: {e}"))?;
    let info = list
        .get("commands")
        .and_then(|c| c.as_array())
        .and_then(|cmds| {
            cmds.iter()
                .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(command))
        })
        .ok_or_else(|| format!("unknown command `{command}` (try: continuum commands/list)"))?;
    println!("{}", render_cli_help(command, info));
    Ok(())
}

/// Render a command's `CommandInfo` (from commands/list) as CLI/bash help. Pure
/// (Value in, String out) so it's unit-testable without a running core.
pub(super) fn render_cli_help(command: &str, info: &Value) -> String {
    let desc = info
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("");
    let mut out = format!("{command} — {desc}\n\n");
    out.push_str(&format!(
        "Usage: continuum {command} [--flag value ...]   (or a single JSON object)\n"
    ));

    let schema = info.get("paramsSchema");
    let required: Vec<&str> = schema
        .and_then(|s| s.get("required"))
        .and_then(|r| r.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    match schema
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.as_object())
    {
        Some(props) if !props.is_empty() => {
            out.push_str("\nParams:\n");
            for (name, spec) in props {
                let ty = schema_type_str(spec);
                let pdesc = spec
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                let req = if required.contains(&name.as_str()) {
                    "  (required)"
                } else {
                    ""
                };
                out.push_str(&format!(
                    "  --{:<22} {:<9} {}{}\n",
                    camel_to_kebab(name),
                    ty,
                    pdesc,
                    req
                ));
            }
        }
        _ => out.push_str("\n(no params)\n"),
    }
    out
}

/// Best-effort JSON-Schema type label for a property (handles `"type":"string"`,
/// `"type":["string","null"]` for optionals, a `$ref` to a nested type, or an
/// unschematized field).
fn schema_type_str(spec: &Value) -> String {
    // A nested struct/enum is a `$ref` (e.g. "#/definitions/Foo") — show its name
    // rather than an opaque <value>.
    if let Some(r) = spec.get("$ref").and_then(|v| v.as_str()) {
        return format!("<{}>", r.rsplit('/').next().unwrap_or(r));
    }
    match spec.get("type") {
        Some(Value::String(s)) => format!("<{s}>"),
        Some(Value::Array(a)) => {
            let first = a
                .iter()
                .filter_map(|v| v.as_str())
                .find(|s| *s != "null")
                .unwrap_or("any");
            format!("<{first}>")
        }
        _ => "<value>".to_string(),
    }
}

/// camelCase → kebab-case for display (`roundTripMs` → `round-trip-ms`). continuum's
/// adapter accepts either form, so the displayed flag is also a valid one.
pub(super) fn camel_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            out.push('-');
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Dispatch through the uniform Connection to an already-running core. Lifecycle is
/// explicit: probing a node during a deploy must never launch its pre-swap image.
/// This applies to every command, including dynamically registered ML commands;
/// there is no read-verb allowlist that can drift from the registry.
pub(super) async fn dispatch(command: &str, args: Vec<String>) -> Result<(), CliError> {
    ensure_core_running(command).await?;
    let canonical = canonical_param_names(command).await;
    let params = params_from_args(command, &args, &canonical)?;
    let result = match connection().commands().execute_value(command, params).await {
        Ok(result) => result,
        // A REFUSAL WITH DATA SHOWS ITS DATA (card f4d2fa49). A handler that answers
        // `{ success: false, errorKind, nextHistoryOffset, malformed, … }` is refusing
        // with the fields the operator needs next; the sentence alone is not enough.
        // The typed outcome goes to stdout as JSON, the refusal line to stderr as
        // before, and the exit code is still the refusal's.
        Err(e) => {
            if let ClientError::Refused {
                outcome: Some(outcome),
                ..
            } = &e
            {
                println!(
                    "{}",
                    serde_json::to_string_pretty(outcome).unwrap_or_else(|_| outcome.to_string()) // boundary: CLI stdout; a Value always encodes, its Display is the same JSON compact
                );
            }
            return Err(format!("{command}: {e}").into());
        }
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
    );
    Ok(())
}

/// Adapt CLI args into command params — the CLI's edge of the uniform
/// param-adaptation principle (meet humans/AIs in the middle at every interface).
/// PROCEDURAL and generic: ONE rule for ALL commands, never a per-command switch.
///
/// Three forms, in precedence:
/// 1. nothing → `{}`.
/// 2. a single positional JSON object/array → used verbatim (the AI / power-user
///    path; same payload a tool call would send).
/// 3. `--key value` / `--flag` pairs → a JSON object, built by one loop:
///    - keys are normalized kebab/snake → camelCase (`--round-trip-ms` →
///      `roundTripMs`), matching the canonical camelCase wire fields;
///    - values are coerced by trying JSON first (`5`→number, `true`→bool,
///      `{...}`→object), falling back to a string — so humans type `--count 5`
///      and the typed command still receives a number;
///    - a bare `--flag` (no following value) is `true`.
///
/// `canonical` is the command's exact param field names (from its `commands/list`
/// schema). A user flag is matched against them separator/case-insensitively, so
/// `--persona_id`, `--persona-id`, `--personaId` all map to the schema's real
/// `persona_id`. This is what makes snake_case Rust-native commands invokable by
/// flag (regression: continuum used to blanket-camelCase every key, turning `--persona_id`
/// into `personaId`, which the server rejected as `missing field persona_id`).
/// A flag NOT in the schema is REFUSED by name — with the command and the flags it
/// does accept — but only when the schema is KNOWN (`canonical` non-empty). With no
/// schema (registry unreachable, or a command that publishes no params) "unknown"
/// is unprovable, so the historical camelCase guess still stands; refusing there
/// would make every command the registry cannot describe uncallable. Measured
/// 2026-09-05: `continuum ping --nonsense-flag` returned a healthy pong, exit 0 —
/// the typo became a junk param the command ignored, and the caller got a
/// successful-looking answer to a question nobody asked (#3724).
///
/// Schema-AWARE coercion/validation (knowing each field's exact type) is the next
/// step on the same `commands/list` schema; this canonicalizes the KEY today.
pub(super) fn params_from_args(
    command: &str,
    args: &[String],
    canonical: &[String],
) -> Result<Value, String> {
    if args.is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    if args.len() == 1 {
        let t = args[0].trim_start();
        if t.starts_with('{') || t.starts_with('[') {
            return serde_json::from_str(&args[0])
                .map_err(|e| format!("invalid JSON params: {e}\n(got: {})", args[0]));
        }
    }

    // {normalized form → exact schema field}. One generic rule for ALL commands.
    let canon_by_norm: std::collections::HashMap<String, &str> = canonical
        .iter()
        .map(|c| (normalize_key(c), c.as_str()))
        .collect();
    // Universally-common flag synonyms. If the user typed one spelling and the
    // SCHEMA uses the other, resolve to the schema's canonical field — so muscle
    // memory ("--command" for a shell, the name Claude's Bash tool + most CLIs use)
    // is served, not bounced (Joel's rule #328: canonical follows the common
    // standard, aliases resolve). Only consulted when the raw flag is NOT itself a
    // canonical field; data-driven, so it never overrides a command's real param.
    const SYNONYMS: &[(&str, &str)] = &[("command", "cmd")];
    // `None` = a flag the command's KNOWN schema does not have — see the last arm.
    let field = |raw: &str| -> Option<String> {
        let norm = normalize_key(raw);
        if let Some(c) = canon_by_norm.get(&norm) {
            return Some((*c).to_string());
        }
        for (a, b) in SYNONYMS {
            let other = if normalize_key(a) == norm {
                Some(*b)
            } else if normalize_key(b) == norm {
                Some(*a)
            } else {
                None
            };
            if let Some(o) = other {
                if let Some(c) = canon_by_norm.get(&normalize_key(o)) {
                    return Some((*c).to_string());
                }
            }
        }
        // NOT a schema field, and not a synonym of one.
        //
        // When `canonical` is EMPTY we do not know the schema — the registry was
        // unreachable, or this command publishes no params — so "unknown" is
        // unprovable and the historical camelCase guess stands. When it is
        // NON-empty we DO know, and a flag matching nothing is a typo the caller
        // wants told about, not silently coerced into a junk key the command
        // ignores. Same unset-vs-unknown ladder as #3717's room probe: refuse only
        // where the knowledge to refuse actually exists.
        if canon_by_norm.is_empty() {
            return Some(to_camel_case(raw));
        }
        None
    };
    // Name the flag AND what the command actually takes. A refusal that only says
    // "unknown" sends the caller to `--help`; one that lists the fields IS the
    // answer they were about to look up.
    let unknown = |k: &str| -> String {
        let mut known: Vec<&str> = canon_by_norm.values().copied().collect();
        known.sort_unstable();
        format!(
            "unknown flag `--{k}` for `{command}`.\n  accepted: {}\n  (or pass a single JSON \
             object; `continuum {command} --help` shows types and which are required)",
            known
                .iter()
                .map(|f| format!("--{f}"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };

    let mut map = serde_json::Map::new();
    let mut i = 0;
    while i < args.len() {
        let raw_key = args[i].strip_prefix("--").ok_or_else(|| {
            format!(
                "expected `--key value` or a single JSON object, got `{}`",
                args[i]
            )
        })?;
        // Support BOTH `--key value` and the muscle-memory `--key=value` form.
        // Splitting on the first `=` means `--filter=data/` parses to
        // {filter: "data/"} instead of a junk `{"filter=data/": true}` key.
        if let Some((k, v)) = raw_key.split_once('=') {
            map.insert(field(k).ok_or_else(|| unknown(k))?, coerce(v));
            i += 1;
            continue;
        }
        // A value follows unless the next arg is another flag (or there is none).
        let has_value = args.get(i + 1).is_some_and(|n| !n.starts_with("--"));
        if has_value {
            map.insert(
                field(raw_key).ok_or_else(|| unknown(raw_key))?,
                coerce(&args[i + 1]),
            );
            i += 2;
        } else {
            map.insert(
                field(raw_key).ok_or_else(|| unknown(raw_key))?,
                Value::Bool(true),
            );
            i += 1;
        }
    }
    Ok(Value::Object(map))
}

/// Normalize a flag key for schema matching: lowercase, separators removed. So
/// `persona_id`, `personaId`, `persona-id`, `PERSONA_ID` share one normal form.
fn normalize_key(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Best-effort fetch of a command's canonical param field names from the live
/// registry (`commands/list` filtered to this one command — cheap, not the full
/// catalog). Returns the exact serde field names (`persona_id`, `roomId`, …) so the
/// arg adapter can canonicalize a user flag to the schema's real name regardless of
/// separator/case. Empty on ANY failure: the adapter then uses its generic
/// camelCase normalization, which is the pre-schema behavior — a command that needs
/// no canonicalization (camelCase fields) still works without the registry.
async fn canonical_param_names(command: &str) -> Vec<String> {
    let list = match connection()
        .commands()
        .execute_value("commands/list", serde_json::json!({ "filter": command }))
        .await
    {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    list.get("commands")
        .and_then(|c| c.as_array())
        .and_then(|cmds| {
            cmds.iter()
                .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(command))
        })
        .and_then(|info| info.get("paramsSchema"))
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.as_object())
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default()
}

/// Coerce a CLI string value: try JSON first (`5`→number, `true`→bool, `{…}`→
/// object), else keep it a string. The one rule for every command, no schema
/// needed (schema-aware coercion is a follow-up once commands/list schemas drive it).
fn coerce(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// kebab/snake → camelCase (`round-trip-ms`/`round_trip_ms` → `roundTripMs`).
pub(super) fn to_camel_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = false;
    for ch in s.chars() {
        if ch == '-' || ch == '_' {
            upper = true;
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}
