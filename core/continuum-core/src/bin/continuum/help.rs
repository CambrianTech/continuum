//! `--help` rendering and the usage text: the CLI adapter for a command's manual.

use super::*;

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
pub(super) fn schema_type_str(spec: &Value) -> String {
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

pub(super) fn usage() -> String {
    "usage: continuum <start|reboot|stop|desktop|command> [json | --key value ...]  (uu = continuum)\n\
     \n\
     Version: continuum --version (also -V or version); identifies this CLI offline\n\
     Lifecycle:\n  \
       continuum start                 build + run the headless Rust core (detached), wait until ready;\n                                       refuses if a core is running but not answering (a second core on\n                                       one socket makes results non-deterministic)\n  \
       continuum start --force         reclaim those unresponsive core(s) first, then start\n  \
       continuum reboot                rebuild + relaunch; verifies the RUNNING core's build SHA\n  \
       continuum reboot --prebuilt <path> [--service | --validate-only]\n                                       validate and launch that core without rebuilding; retains cwd\n                                       and matches checkout HEAD when run in a repository\n                                       Windows --service uses the installer's prepared task;\n                                       --validate-only checks without stopping or launching\n                                       add --embedding-model <GGUF> for bounded CPU model validation\n  \
       continuum maintenance <1..300>   bounded durable serving pause, then restore (Windows)\n  \
       continuum maintenance recover    restore interrupted scheduler maintenance\n  \
       continuum stop                  stop the core; the engine, its weights and every mind's KV stay\n                                       up for the next `continuum start` to adopt (no model reload)\n  \
       continuum stop --engine         stop the core and the engine (frees the engine's memory)\n  \
       continuum deploy-verify         prove the running core's build SHA matches the deployed source\n  \
       continuum install [--check]     converge this machine: the OS supervisor (Windows: S4U at boot +\n                                       the deploy consumer, one elevation; macOS: the system LaunchDaemon,\n                                       sudo once, --user = the agent), the core (build HEAD, stage, hand\n                                       off when the running build is not HEAD), the CLI on PATH\n                                       (continuum + uu follow the slot), and airc (installed, started at\n                                       login). Each arm reads, changes only what drifted, says so.\n                                       --check reads only. Name arms with --supervisor --core --cli\n                                       --airc. (linux arms pending)\n  \
       continuum uninstall             unregister the supervisor job (the staged binary stays)\n  \
       continuum supervisor-status [--crash-test]\n                                       who owns the running core; --crash-test = kill -9, expect a heal < 60 s\n\
     \n\
     Legacy checkpoint recovery (local; no running core required):\n  \
       continuum checkpoint inspect --source <volatile.json> --persona-id <uuid> --plan <new-file>\n                                       save an explicit digest-bound selection; no checkpoint changed\n  \
       continuum checkpoint adopt --plan <file> --legacy-writers-stopped\n                                       preserve both snapshots and adopt the selected bytes offline;\n                                       stop legacy cores and automatic launchers first; no final-flush claim\n  \
       continuum engine root                print the managed engine root\n  \
       continuum engine idle-slot --owner-pid <pid> [--exclude-slot engine-a|engine-b|engine-c]...\n                                       claim and print a lane-safe slot for builder <pid> (exit 3: none idle)\n  \
       continuum engine release-slot <slot> --owner-pid <pid>\n                                       release builder <pid>'s claim on a slot\n  \
       continuum engine promote <slot> <commit:backend>\n                                       make a verified slot the current engine\n  \
       continuum engine attempt <slot>       inspect admitted artifact and failure disposition\n  \
       continuum deploy-consume --retry-resource <build-sha> <condition-sha256>\n                                       authorize one exact saved resource retry\n  \
       continuum engine retry <slot> <commit:backend> <artifact-identity>\n                                       explicitly retry exactly that artifact\n  \
       continuum engine rollback <failed-slot>\n                                       put the previous engine back while <failed-slot> is current\n\
     \n\
     Desktop (the core serves it; no port to remember):\n  \
       continuum desktop               open the desktop in your browser (alias: uu desktop)\n\
     \n\
     Commands (dispatch to the running core):\n  \
       continuum ping\n  \
       continuum ping --message hi                 # --key value, coerced + camelCased automatically\n  \
       continuum ping '{\"message\":\"hi\"}'           # or a single JSON object (AI / power-user path)\n  \
       continuum commands/list                     # discover commands dynamically (single source)\n  \
       continuum commands/list --filter data/\n\
     \n\
     Env: CONTINUUM_CORE_SOCKET (default /tmp/continuum-core.sock)\n     \
          CONTINUUM_START_SCRIPT (override the start script path)"
        .to_string()
}
