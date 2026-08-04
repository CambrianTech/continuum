//! `continuum` — the pure-Rust Continuum CLI: the ONE surface for both lifecycle and
//! commands. Replaces the legacy Node `./jtag` and the bare start scripts.
//!
//! ```text
//! continuum start            # build + run the headless Rust core (detached), wait until ready
//! continuum reboot           # rebuild + relaunch, replacing any running core (~0 downtime)
//!                     # refuses while training (mlx_lm) is live — `--force` overrides
//! continuum stop             # stop the running core
//! continuum ping             # dispatch a command to the running core
//! continuum ping '{"message":"hi"}'
//! continuum data/list '{"collection":"users"}'
//! ```
//!
//! Lifecycle (`start`/`stop`) wraps the pure-Rust `tools/scripts/start-server.sh`
//! (the implementation detail: cargo-run the core with per-platform GPU features,
//! no Node). Commands dispatch through the SAME uniform [`Connection`] every client
//! uses (CLI/persona/web/mobile) over the core IPC socket via [`CoreIpcTransport`].
//! No tsx, no bundle, no Node anywhere.
//!
//! Env: `CONTINUUM_CORE_SOCKET` (default `/tmp/continuum-core.sock`),
//! `CONTINUUM_START_SCRIPT` (override the start script path).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use continuum_client::Connection;
use continuum_core::runtime::core_ipc_transport::CoreIpcTransport;
use serde_json::Value;

/// Where `continuum start` records the detached core's PID so `continuum stop` can find it.
fn pidfile_for(socket: &str) -> String {
    format!("{socket}.pid")
}
fn start_logfile() -> String {
    continuum_core::ipc::endpoint_paths::core_start_logfile()
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("continuum: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let first = args.next().ok_or_else(usage)?;
    match first.as_str() {
        "-h" | "--help" | "help" => {
            eprintln!("{}", usage());
            Ok(())
        }
        "start" => start().await,
        "reboot" | "restart" => {
            let force = args.any(|a| a == "--force");
            reboot(force).await
        }
        "stop" => stop().await,
        // Standalone #194 check: prove the RUNNING core is built from current HEAD,
        // without a full reboot. Prints "✅ deploy verified" or fails loud on mismatch.
        "deploy-verify" | "verify" => verify_deployed_build().await,
        // Anything else is a command name. `--help`/`-h` renders the manual in the
        // CLI's paradigm (bash flags), adapted from the SAME schema the AI gets as
        // a tool spec. Otherwise dispatch, params adapted procedurally.
        command => {
            // Meet the operator's dialect: `continuum read_file ...` / `continuum code_read ...`
            // resolve to the canonical `code/read` through the SAME tool_dialect
            // section personas and the socket route use — so the CLI accepts the
            // same vocabulary, and help + param-adaptation below key off the real
            // command name. Idempotent for an already-canonical name.
            let command = continuum_core::cognition::tool_dialect::resolve_wire_name(command);
            let rest: Vec<String> = args.collect();
            if rest.iter().any(|a| a == "--help" || a == "-h") {
                help_for(&command).await
            } else {
                dispatch(&command, rest).await
            }
        }
    }
}

/// `continuum <command> --help` — the CLI adapter for the command's manual: query the
/// live registry (`commands/list`) for the command's description + params schema,
/// then render it as bash usage. Same single source the AI tool adapter reads;
/// only the rendering differs by paradigm ("the manual matches the paradigm").
async fn help_for(command: &str) -> Result<(), String> {
    ensure_core_running(command).await?;   // the manual comes from the live registry
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
fn render_cli_help(command: &str, info: &Value) -> String {
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
fn camel_to_kebab(s: &str) -> String {
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

fn socket_path() -> String {
    continuum_core::ipc::endpoint_paths::core_socket_path()
}

/// Dispatch a single command to the core through the uniform Connection, STARTING the
/// core first if nothing is answering.
///
/// Why auto-start is infrastructure and not convenience: every governed long-running
/// operation lives behind a command (`models/pull` resumes, is content-addressed,
/// journals to `~/.continuum/progress/`, emits progress on the bus). None of that is
/// reachable when the core is down, so "the core isn't running" turns into a bare
/// `nohup <downloader> &` — which has no ledger, no resume, no progress, and dies
/// silently leaving an empty directory. That has now happened to two separate
/// multi-hour model pulls (K3, then V4-Flash IQ1_S) and cost days.
///
/// The governed path must be the path of LEAST resistance or it does not get used.
/// One ping decides; `continuum start` is already idempotent and waits for ready.
/// Set `CONTINUUM_NO_AUTOSTART=1` where spawning a core is not acceptable (CI, probes)
/// — it then fails with the reason rather than silently doing nothing.
async fn dispatch(command: &str, args: Vec<String>) -> Result<(), String> {
    ensure_core_running(command).await?;
    let canonical = canonical_param_names(command).await;
    let params = params_from_args(&args, &canonical)?;
    let result = connection()
        .commands()
        .execute_value(command, params)
        .await
        .map_err(|e| format!("{command}: {e}"))?;
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
/// Flags NOT in the schema — base `CommandParams` like userId, or commands that
/// expose no schema (`canonical` empty) — fall back to the generic camelCase
/// normalization, which is correct for the camelCase wire fields and is the
/// pre-schema behavior, so nothing regresses.
///
/// Schema-AWARE coercion/validation (knowing each field's exact type) is the next
/// step on the same `commands/list` schema; this canonicalizes the KEY today.
fn params_from_args(args: &[String], canonical: &[String]) -> Result<Value, String> {
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
    let field = |raw: &str| -> String {
        canon_by_norm
            .get(&normalize_key(raw))
            .map(|c| (*c).to_string())
            .unwrap_or_else(|| to_camel_case(raw))
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
            map.insert(field(k), coerce(v));
            i += 1;
            continue;
        }
        // A value follows unless the next arg is another flag (or there is none).
        let has_value = args.get(i + 1).is_some_and(|n| !n.starts_with("--"));
        if has_value {
            map.insert(field(raw_key), coerce(&args[i + 1]));
            i += 2;
        } else {
            map.insert(field(raw_key), Value::Bool(true));
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
fn to_camel_case(s: &str) -> String {
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

fn connection() -> Connection<CoreIpcTransport> {
    Connection::new(CoreIpcTransport::new(socket_path()))
}

/// Is a core already answering on the socket? A real ping round-trip, not just a
/// socket-file existence check (a stale socket file lies).
async fn core_is_up() -> bool {
    matches!(
        connection()
            .commands()
            .execute_value("ping", Value::Object(Default::default()))
            .await,
        Ok(_)
    )
}

/// Make sure a core is answering before dispatching, launching one if not. See the
/// `dispatch` doc for why this is load-bearing rather than a nicety.
///
/// Announces on stderr (never stdout — stdout is the command's JSON result and stays
/// machine-parseable) so an operator who typed one command and got a 60s pause knows
/// exactly what is happening instead of assuming it hung.
async fn ensure_core_running(command: &str) -> Result<(), String> {
    if core_is_up().await {
        return Ok(());
    }
    if std::env::var("CONTINUUM_NO_AUTOSTART").is_ok_and(|v| v != "0") {
        return Err(format!(
            "no core is answering on {} and CONTINUUM_NO_AUTOSTART is set, so `{command}` \
             cannot be dispatched. Start one with `continuum start`.",
            socket_path()
        ));
    }
    eprintln!("▶ no core running — starting one for `{command}` (continuum start)");
    let secs = launch_core(&[]).await.map_err(|e| {
        format!("`{command}` needs a running core and one could not be started: {e}")
    })?;
    eprintln!("✅ core ready after ~{secs}s — dispatching `{command}`");
    Ok(())
}

/// `continuum start` — build + run the headless Rust core (detached), wait until it
/// answers `ping`. Idempotent: a no-op if a core is already up.
async fn start() -> Result<(), String> {
    let socket = socket_path();

    if core_is_up().await {
        println!("core already running (socket={socket})");
        return Ok(());
    }

    let secs = launch_core(&[]).await?;
    println!("✅ core ready (socket={socket}) after ~{secs}s");
    Ok(())
}

/// `continuum reboot` — rebuild + relaunch the core, replacing any running instance.
/// Unlike `continuum start` this never no-ops on an up core: start-server.sh builds the
/// fresh binary first (old core keeps serving), then stops the old core and
/// execs the new one (~0 downtime). This is the canonical operator
/// rebuild-after-edit verb — one command, no manual kill dance
/// ([[validate-via-pure-rust-not-npm-jtag]]).
async fn reboot(force: bool) -> Result<(), String> {
    let socket = socket_path();
    // Training guard (task #137, Joel's consent-gate doctrine: the denial names
    // the policy AND the path). A core swap kills spawned trainer children
    // (mlx_lm.lora) and their in-process watchers — glass-boxed 2026-07-11: 41
    // jobs submitted, zero outcomes recorded, all orphaned by reboots. Live
    // training is a lease the reboot must respect, not silently revoke.
    let trainers = running_trainer_pids();
    if !trainers.is_empty() && !force {
        return Err(format!(
            "training in flight (mlx_lm pid(s) {}) — a reboot would kill it and the \
             run would be journaled killed-by-reboot. Wait for it to finish, or rerun \
             with `continuum reboot --force` if losing the run is acceptable.",
            trainers
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if !trainers.is_empty() {
        println!(
            "⚠ --force: rebooting over live training (mlx_lm pid(s) {}) — the run dies \
             here and the job ledger will record killed-by-reboot at next boot",
            trainers
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    // Snapshot the running core PIDs up front so launch_core can wait for them to
    // actually exit before trusting the new core's ping (same socket, both answer).
    let old = running_core_pids();
    if old.is_empty() {
        println!("▶ no core running — starting fresh (socket={socket})");
    } else {
        println!(
            "▶ rebooting core (socket={socket}, replacing pid(s) {}) — building fresh binary, then swapping",
            old.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
        );
    }
    let secs = launch_core(&old).await?;
    // Deploy-verification (#194): a new core is up — but is it the FRESHLY-BUILT one? If
    // start-server.sh's build was a stale cache no-op or silently failed, an OLD binary would
    // answer on the same socket and this reboot would report success while running dead code.
    // NO success line before provenance is proven ("core ready" without provenance is a false
    // deploy receipt — the 2026-08-01 Windows-node incident): announce liveness neutrally,
    // then verify, and let "✅ deploy verified" be the ONLY checkmark a reboot prints.
    println!("core answering (socket={socket}) after ~{secs}s — verifying deploy provenance (#194)");
    verify_deployed_build().await
}

/// Prove the running core is built from the source this deploy shipped — the honest half of
/// "reboot succeeded". The RUNNING core self-reports its compiled-in SHA over the socket
/// (`ping` → `buildSha`): the live process image answers for itself, so this can't be fooled
/// by re-exec'ing an on-disk file a rebuild already swapped under the still-running old core,
/// and it needs no path guessing at all (the 2026-08-01 Windows-node incident: a "next to the
/// CLI exe" guess printed "could not locate continuum-core-server" while a 2-day-old binary
/// kept serving — and the reboot still said success).
///
/// Expected SHA precedence:
/// 1. git HEAD of the checkout the reboot built from (start-server.sh's freshness guard
///    already pins artifact == source at build time), when run inside a git tree;
/// 2. otherwise the deployable artifact resolved by the SAME order install-service.sh uses
///    (`CONTINUUM_CORE_BIN` env → installed locations → cargo target dir), asked for its
///    `--build-sha` — the installed-node path, where there is no git tree.
///
/// NEVER skips soft: any gap (no core answering, a pre-#194 core without `buildSha`,
/// `unknown` provenance, no resolvable artifact) is an ERROR — a reboot must not print a
/// success line it cannot back with provenance ([[fallbacks-are-illegal-fail-loud]]).
async fn verify_deployed_build() -> Result<(), String> {
    let socket = socket_path();
    // The RUNNING core's provenance, from the process itself.
    let reply = connection()
        .commands()
        .execute_value("ping", Value::Object(Default::default()))
        .await
        .map_err(|e| format!("deploy-verify: no core answering on {socket}: {e}"))?;
    let actual = reply
        .get("buildSha")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // What the deploy SHOULD have shipped.
    let (expected, expected_source) = match git_head_short_sha() {
        Some(head) => (head, "git HEAD of this checkout".to_string()),
        None => {
            let artifact = resolve_core_artifact()?;
            let sha = binary_build_sha(&artifact)?;
            (sha, format!("artifact {}", artifact.display()))
        }
    };

    let running_desc = describe_running_core(&socket);
    match deploy_verdict(actual.as_deref(), &expected, &expected_source, &running_desc) {
        Ok(line) => {
            println!("{line}");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// The pure compare at the heart of deploy-verify: running core's self-reported SHA vs the
/// SHA the deploy shipped. Ok(success line) only on a REAL match; every gap is a loud error
/// naming both SHAs and both identities. Pure (strings in, strings out) so it's unit-testable
/// without a running core.
fn deploy_verdict(
    actual: Option<&str>,
    expected: &str,
    expected_source: &str,
    running_desc: &str,
) -> Result<String, String> {
    let actual = match actual {
        Some(a) if !a.is_empty() => a,
        _ => {
            return Err(format!(
                "DEPLOY MISMATCH (#194): the running core ({running_desc}) does not report a build \
                 SHA on ping — it is a pre-#194 (or otherwise stale) binary, so the swap did NOT \
                 happen. Expected build {expected} ({expected_source}). Do not trust any live test: \
                 stop the old core and reboot again."
            ))
        }
    };
    if actual == "unknown" || expected == "unknown" {
        return Err(format!(
            "DEPLOY UNVERIFIABLE (#194): build provenance is 'unknown' (running core \
             ({running_desc}) reports {actual}; expected {expected} from {expected_source}) — a \
             binary was built outside a git tree, so freshness cannot be proven. Rebuild inside \
             the git checkout and reboot again; never trust an unverifiable deploy."
        ));
    }
    if sha_matches(actual, expected) {
        Ok(format!(
            "✅ deploy verified: core is running build {actual} (== {expected_source})"
        ))
    } else {
        Err(format!(
            "DEPLOY MISMATCH (#194): the running core ({running_desc}) is build {actual}, but the \
             deploy shipped build {expected} ({expected_source}). The swap did NOT take — a \
             stale binary is still serving while the reboot would have claimed success. Do not \
             trust any live test until this is fixed: rebuild cleanly (`cargo build -p \
             continuum-core --bin continuum-core-server`) and reboot again."
        ))
    }
}

/// Two short/long git SHAs refer to the same commit when one prefixes the other (git's
/// `--short` abbreviation length varies over a repo's life). Both must be real hex SHAs of
/// credible length — never matches `""` or `"unknown"`.
fn sha_matches(a: &str, b: &str) -> bool {
    let credible =
        |s: &str| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit());
    credible(a) && credible(b) && (a.starts_with(b) || b.starts_with(a))
}

/// Best-effort human identity of the running core for error messages: socket + pid(s) +
/// process image path where resolvable. Diagnostics ONLY — the SHA itself always comes from
/// the process over the socket, never from re-executing a path guessed here.
fn describe_running_core(socket: &str) -> String {
    let pids = running_core_pids();
    let mut desc = format!("socket={socket}");
    if !pids.is_empty() {
        desc.push_str(&format!(
            ", pid(s) {}",
            pids.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",")
        ));
    }
    if let Some(img) = running_core_binary() {
        desc.push_str(&format!(", image {}", img.display()));
    }
    desc
}

/// Resolve the deployable `continuum-core-server` artifact — the SAME order
/// `tools/scripts/install-service.sh::resolve_core_bin` uses, centralized so the CLI and the
/// service installer can never disagree: `CONTINUUM_CORE_BIN` env → installed locations
/// (`/usr/local/bin`, `~/.continuum/bin`) → cargo target dir (release, then debug; default
/// target dir `~/.continuum/cache/cargo-target`, matching start-server.sh). NEVER "next to
/// the CLI exe" — that guess is what printed "could not locate continuum-core-server" on the
/// 2026-08-01 Windows node while a stale core kept serving.
fn resolve_core_artifact() -> Result<PathBuf, String> {
    let env_bin = std::env::var("CONTINUUM_CORE_BIN").ok();
    if let Some(explicit) = &env_bin {
        let p = PathBuf::from(explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("CONTINUUM_CORE_BIN={explicit} is not a file"));
    }
    let home = home_dir()?;
    let target = std::env::var("CARGO_TARGET_DIR").ok();
    let candidates = core_artifact_candidates(&home, target.as_deref());
    candidates
        .iter()
        .find(|p| p.is_file())
        .cloned()
        .ok_or_else(|| {
            format!(
                "no continuum-core-server artifact found. Searched (install-service.sh order): {}. \
                 Build one (`continuum reboot` from the repo, or npm start) or set CONTINUUM_CORE_BIN.",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// The ordered candidate list behind [`resolve_core_artifact`] — pure (paths in, paths out)
/// so the shared resolution ORDER is pinned by a unit test against install-service.sh.
fn core_artifact_candidates(home: &str, cargo_target_dir: Option<&str>) -> Vec<PathBuf> {
    let exe = if cfg!(windows) {
        "continuum-core-server.exe"
    } else {
        "continuum-core-server"
    };
    let target = cargo_target_dir
        .map(str::to_string)
        .unwrap_or_else(|| format!("{home}/.continuum/cache/cargo-target"));
    let mut out = Vec::new();
    if !cfg!(windows) {
        out.push(PathBuf::from("/usr/local/bin").join(exe));
    }
    out.push(PathBuf::from(home).join(".continuum").join("bin").join(exe));
    out.push(PathBuf::from(&target).join("release").join(exe));
    out.push(PathBuf::from(&target).join("debug").join(exe));
    out
}

/// The user's home dir — `HOME` (unix) or `USERPROFILE` (Windows). Loud when absent: the
/// resolution order depends on it, and guessing would defeat the shared contract.
fn home_dir() -> Result<String, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| {
            "HOME/USERPROFILE unset — cannot resolve installed continuum-core-server locations \
             (set CONTINUUM_CORE_BIN explicitly)"
                .to_string()
        })
}

/// Ask an on-disk `continuum-core-server` artifact for its embedded build SHA
/// (`--build-sha`, exits before any socket/side-effect). Loud on any failure — an artifact
/// that cannot state its provenance cannot anchor a deploy receipt.
fn binary_build_sha(artifact: &Path) -> Result<String, String> {
    let out = std::process::Command::new(artifact)
        .arg("--build-sha")
        .output()
        .map_err(|e| format!("cannot run {} --build-sha: {e}", artifact.display()))?;
    if !out.status.success() {
        return Err(format!(
            "{} --build-sha exited {} — a pre-#194 artifact cannot anchor a deploy receipt; rebuild it",
            artifact.display(),
            out.status
        ));
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(format!(
            "{} --build-sha printed nothing — rebuild the artifact",
            artifact.display()
        ));
    }
    Ok(sha)
}

/// Short git HEAD SHA of the current checkout (matches what `build.rs` embeds), or `None`
/// when not in a git tree.
fn git_head_short_sha() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Path to the executable image of the ACTUALLY-RUNNING core, resolved from its live pid.
/// DIAGNOSTICS ONLY (error-message context via [`describe_running_core`]): the deploy receipt
/// itself never re-execs this path — after a rebuild the on-disk file at the running pid's
/// path is already the NEW binary while the old image keeps serving, so exec'ing it
/// false-passes a stale deploy (the inverse of the 2026-07-25 release-leftover false alarm).
/// The running core's SHA always comes from the process itself over the socket (`ping`).
fn running_core_binary() -> Option<std::path::PathBuf> {
    let pid = running_core_pids().into_iter().next()?;
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS/BSD have no /proc; `ps -o comm=` prints the full executable path.
        let out = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .ok()
            .filter(|o| o.status.success())?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!path.is_empty()).then(|| std::path::PathBuf::from(path))
    }
}

/// PIDs of live training runs (`mlx_lm` trainers spawned by the MLX adapter) —
/// the reboot guard's evidence. Same pgrep shape as [`running_core_pids`].
fn running_trainer_pids() -> Vec<i32> {
    pgrep("mlx_lm")
}

/// PIDs of every running `continuum-core-server`, via `pgrep` (pure unix, no
/// Node). Empty on no match or if pgrep is unavailable.
fn running_core_pids() -> Vec<i32> {
    pgrep("continuum-core-server")
}

fn pgrep(pattern: &str) -> Vec<i32> {
    std::process::Command::new("pgrep")
        .args(["-f", pattern])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<i32>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// True if `pid` is still alive. Unix: signal 0 is the canonical liveness probe.
/// Windows has no signals — query the task list for the pid.
fn pid_alive(pid: i32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: kill(pid, 0) sends no signal; it only checks existence/permission.
        unsafe { libc::kill(pid, 0) == 0 }
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

/// Spawn the pure-Rust start script detached and wait until the core answers
/// `ping`. Shared by `continuum start` (after an up-check) and `continuum reboot` (always).
///
/// `wait_for_death` is the set of core PIDs that must EXIT before we trust the
/// ping. Without it, `continuum reboot` would see the OLD core still answering on the
/// same socket and falsely report "ready" before the swap happened — a fail-loud
/// violation that would also hide a failed rebuild. `continuum start` passes `&[]`.
///
/// Returns the seconds waited. Deliberately prints NO success line — the caller owns the
/// receipt (#194): `start` may celebrate liveness, but `reboot` must verify deploy
/// provenance first and only then print its one checkmark.
/// Resolve the bash that runs the start script.
///
/// `Command::new("bash")` is WRONG on Windows: PATH lookup finds `C:\Windows\System32\bash.exe`,
/// which is the WSL launcher, not a POSIX shell. It hands the script to a Linux distro that may
/// not exist and dies with `execvpe(/bin/bash) failed: No such file or directory`. That is exactly
/// what `continuum start` has been doing here — so the core could never start on Windows, so no
/// governed command was reachable, so every long-running job got hand-rolled instead.
///
/// Order: explicit `CONTINUUM_BASH` override, then the Git-for-Windows locations, then a PATH scan
/// that SKIPS the System32 WSL shim. Fails loud and names the fix rather than falling back to a
/// bash that will not work.
fn locate_bash() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("CONTINUUM_BASH") {
        let p = PathBuf::from(&explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("CONTINUUM_BASH is set to `{explicit}` but that is not a file"));
    }

    if !cfg!(windows) {
        return Ok(PathBuf::from("bash"));
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    for env_key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Ok(root) = std::env::var(env_key) {
            let base = PathBuf::from(root);
            candidates.push(base.join("Git").join("bin").join("bash.exe"));
            candidates.push(base.join("Programs").join("Git").join("bin").join("bash.exe"));
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            // The System32 entry is the WSL shim; taking it is the bug this function exists for.
            let lower = dir.to_string_lossy().to_lowercase();
            if lower.contains("system32") {
                continue;
            }
            candidates.push(dir.join("bash.exe"));
        }
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| {
            "no usable bash found. The start script is a bash script and Windows' \
             System32\\bash.exe is the WSL launcher, not a POSIX shell. Install Git for Windows \
             (which provides bash), or point CONTINUUM_BASH at a bash.exe."
                .to_string()
        })
}

async fn launch_core(wait_for_death: &[i32]) -> Result<u64, String> {
    let socket = socket_path();
    let script = locate_start_script()?;
    let logfile = start_logfile();
    let log = std::fs::File::create(&logfile)
        .map_err(|e| format!("cannot open start log {logfile}: {e}"))?;
    let log_err = log
        .try_clone()
        .map_err(|e| format!("cannot clone start log handle: {e}"))?;

    // stderr, not stdout: stdout carries the dispatched command's JSON result and has to stay
    // machine-parseable when a command auto-starts the core on its way through.
    eprintln!("▶ starting core via {} (log: {logfile})", script.display());

    // Spawn the pure-Rust start script in its OWN session (setsid) so it survives
    // `continuum` exiting — a detached daemon, not a child tied to this process.
    let mut cmd = std::process::Command::new(locate_bash()?);
    cmd.arg(&script)
        .env("CONTINUUM_CORE_SOCKET", &socket)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    // Detach so the core outlives this CLI invocation. Unix: setsid() in the
    // forked child before exec, off continuum's session/controlling terminal. Windows:
    // a new process group + detached process (no console tie).
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        // NOT DETACHED_PROCESS. That flag gives the child NO console at all, and the start script
        // is bash — a console-subsystem program that needs one. Under DETACHED_PROCESS it died
        // with exit 1 in ~2s having written nothing at all, which is indistinguishable from "the
        // script is broken" and is why the core appeared to be unstartable on Windows.
        // CREATE_NO_WINDOW gives it a console with no visible window, so it runs normally and its
        // redirected stdout/stderr still land in the start log. Survival past this CLI exiting does
        // not need detachment on Windows: a child is not killed when its parent exits, and
        // CREATE_NEW_PROCESS_GROUP already keeps our Ctrl+C from reaching it.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "failed to spawn `bash {}`: {e}. The start script is bash; on Windows that needs \
             bash on PATH (Git Bash).",
            script.display()
        )
    })?;

    // Record the PID so `continuum stop` can find the detached process group.
    let pidfile = pidfile_for(&socket);
    let _ = std::fs::write(&pidfile, child.id().to_string());

    // Wait until the core answers ping AND every old core PID has exited. The
    // first build (cargo) can take minutes; poll generously, then fail loud with
    // the log tail rather than hang forever. The death-check is what makes a
    // reboot's success signal honest: the ping must come from the NEW core.
    // A COLD start legitimately takes far longer than the old 300s ceiling: the script builds
    // llama-server (CUDA) and then the core, which is tens of minutes on a first run. 300s was not
    // a safety margin, it was a guaranteed false failure on any clean checkout. Raising it is only
    // safe because a DEAD script is now detected within one 2s tick below, so the long ceiling
    // only ever applies to a build that is genuinely still making progress.
    const TICK_SECS: u64 = 2;
    const MAX_WAIT_SECS: u64 = 30 * 60;
    let mut last_progress = String::new();
    for i in 0..(MAX_WAIT_SECS / TICK_SECS) {
        tokio::time::sleep(Duration::from_secs(TICK_SECS)).await;
        let old_still_alive = wait_for_death.iter().any(|p| pid_alive(*p));
        if !old_still_alive && core_is_up().await {
            return Ok((i + 1) * TICK_SECS);
        }
        // Show the build advancing. A multi-minute silent wait is indistinguishable from a hang,
        // and guessing which one you are in is how a long build gets killed and hand-worked around.
        if (i + 1) % 15 == 0 {
            let line = tail(&logfile, 1).trim().to_string();
            if !line.is_empty() && line != last_progress {
                eprintln!("  … {line}");
                last_progress = line;
            }
        }
        // The start script EXITING is the fast, certain answer, and not checking for it was the
        // defect: a script that died in 200ms was indistinguishable from one still doing a
        // multi-minute cargo build, so every startup failure cost the full 300s and then reported
        // a log tail. Observed shape: 300s wait, empty log, no cause -- which is how "the core
        // never starts on this box" stayed invisible long enough to make hand-rolled downloads
        // feel like the only option.
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "the start script exited ({status}) after ~{}s without the core coming up.\n{}",
                (i + 1) * 2,
                start_log_report(&logfile)
            ));
        }
    }
    Err(format!(
        "core did not become ready within 300s (start script still running).\n{}",
        start_log_report(&logfile)
    ))
}

/// Render the start log for a failure message, and say so plainly when there is nothing in it.
/// "Last log lines:" followed by an empty string is worse than no diagnostic at all: it reads as
/// "the log had nothing interesting" when the truth is "the script produced no output whatsoever",
/// which is itself the strongest clue available (it never got far enough to print).
fn start_log_report(logfile: &str) -> String {
    let t = tail(logfile, 20);
    if t.trim().is_empty() {
        let exists = Path::new(logfile).exists();
        format!(
            "The start log {logfile} is {} -- the script produced NO output at all, so it failed \
             before reaching its first message. Run it directly to see why:\n  bash {}",
            if exists { "empty" } else { "missing" },
            locate_start_script()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "tools/scripts/start-server.sh".to_string())
        )
    } else {
        format!("Last log lines:\n{t}")
    }
}

/// `continuum stop` — stop the running core (the detached session started by `continuum start`).
async fn stop() -> Result<(), String> {
    let socket = socket_path();
    let pidfile = pidfile_for(&socket);

    let mut stopped = false;
    if let Ok(contents) = std::fs::read_to_string(&pidfile) {
        if let Ok(pid) = contents.trim().parse::<i32>() {
            // Reap the core + its children. Unix: signal the whole process group
            // (negative pid) — the start script's setsid made the core a group
            // leader. Windows: taskkill /T kills the process tree.
            #[cfg(unix)]
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
                libc::kill(pid, libc::SIGTERM);
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output();
            }
            stopped = true;
            println!("stopping core (pid {pid})");
        }
        let _ = std::fs::remove_file(&pidfile);
    }

    if !stopped {
        // No pidfile (started another way). Fall back to a targeted pkill — still
        // pure unix, no Node.
        let killed = std::process::Command::new("pkill")
            .args(["-f", "continuum-core-server"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if killed {
            println!("stopped continuum-core-server (pkill)");
        } else {
            println!("no running core found");
        }
    }

    let _ = std::fs::remove_file(&socket);
    Ok(())
}

/// Find `tools/scripts/start-server.sh`: an explicit `CONTINUUM_START_SCRIPT`
/// override, else walk up from the cwd until the repo's script is found.
fn locate_start_script() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("CONTINUUM_START_SCRIPT") {
        let p = PathBuf::from(&explicit);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("CONTINUUM_START_SCRIPT={explicit} is not a file"));
    }
    let rel = Path::new("tools/scripts/start-server.sh");
    let mut dir = std::env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    loop {
        let candidate = dir.join(rel);
        if candidate.is_file() {
            return Ok(candidate);
        }
        if !dir.pop() {
            return Err(
                "could not find tools/scripts/start-server.sh by walking up from the \
                 cwd. Run `continuum start` from inside the repo, or set CONTINUUM_START_SCRIPT."
                    .to_string(),
            );
        }
    }
}

/// Last `n` lines of a file (best-effort; for error context).
fn tail(path: &str, n: usize) -> String {
    let Ok(content) = std::fs::read_to_string(path) else {
        return format!("(could not read {path})");
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: the PROCEDURAL param adapter — one generic rule for all
    // commands, no per-command switch. Covers the three forms + coercion +
    // kebab/snake→camelCase + bare flags. This is the CLI edge of "meet humans/AIs
    // in the middle at every interface" (Joel 2026-06-21).
    #[test]
    fn params_adapt_procedurally_from_args() {
        // No schema available → generic camelCase normalization (pre-schema behavior).
        let no_schema: &[String] = &[];

        // 1. nothing → empty object
        assert_eq!(params_from_args(&[], no_schema).unwrap(), json!({}));

        // 2. positional JSON verbatim (the AI / tool-call path)
        assert_eq!(
            params_from_args(&[r#"{"message":"hi"}"#.to_string()], no_schema).unwrap(),
            json!({ "message": "hi" })
        );

        // 3. --key value with coercion: string stays string, number→number,
        //    bool→bool; keys camelCased from kebab/snake.
        let p = params_from_args(
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
            params_from_args(&["--verbose".into()], no_schema).unwrap(),
            json!({ "verbose": true })
        );

        // `--key=value` form (muscle memory) — split on first `=`, NOT a junk key.
        assert_eq!(
            params_from_args(&["--filter=data/".into()], no_schema).unwrap(),
            json!({ "filter": "data/" }),
            "--key=value splits correctly (regression: was {{\"filter=data/\": true}})"
        );
        assert_eq!(
            params_from_args(&["--round-trip-ms=5".into()], no_schema).unwrap(),
            json!({ "roundTripMs": 5 }),
            "--key=value coerces + camelCases"
        );

        // a non-flag, non-JSON arg is a clear error (not silently swallowed)
        assert!(params_from_args(&["oops".into()], no_schema).is_err());
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
            let p = params_from_args(&[spelling.into(), "abc".into()], &canonical).unwrap();
            assert_eq!(p, json!({ "persona_id": "abc" }), "{spelling} → persona_id");
        }
        // `--key=value` form canonicalizes too
        assert_eq!(
            params_from_args(&["--eval-set=x.jsonl".into()], &canonical).unwrap(),
            json!({ "eval_set": "x.jsonl" })
        );
        // a flag NOT in the schema falls back to camelCase (base fields — no regression)
        assert_eq!(
            params_from_args(&["--room-id".into(), "r1".into()], &canonical).unwrap(),
            json!({ "roomId": "r1" }),
            "non-schema flag → legacy camelCase"
        );
        // with no schema at all, everything is legacy camelCase
        assert_eq!(
            params_from_args(&["--persona-id".into(), "abc".into()], &[]).unwrap(),
            json!({ "personaId": "abc" }),
            "no schema → legacy camelCase (pre-schema behavior preserved)"
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

    // what this catches (#194, 2026-08-01 Windows-node incident): the deploy receipt must
    // NEVER pass soft. Every gap — a running core that reports no buildSha (pre-#194 =
    // stale), an 'unknown' provenance, an outright SHA mismatch — is an ERROR naming both
    // SHAs and both identities, and the success line only appears on a REAL match. A
    // regression that turns any of these back into a warning + Ok re-creates "core ready"
    // as a false deploy receipt.
    #[test]
    fn deploy_verdict_never_passes_soft() {
        let running = "socket=/tmp/x.sock, pid(s) 42, image /t/debug/continuum-core-server";

        // real match (including short-vs-long SHA abbreviation drift) → the ONE success line
        let ok = deploy_verdict(Some("abc123f"), "abc123f", "git HEAD of this checkout", running)
            .expect("matching SHAs verify");
        assert!(ok.contains("✅ deploy verified"), "got {ok}");
        assert!(ok.contains("abc123f"), "names the build: {ok}");
        assert!(
            deploy_verdict(Some("abc123f00d"), "abc123f", "src", running).is_ok(),
            "prefix-tolerant across git --short abbreviation drift"
        );

        // mismatch → loud, names BOTH SHAs and BOTH identities, never a success glyph
        let err = deploy_verdict(Some("dead111"), "beef222", "artifact /usr/local/bin/x", running)
            .expect_err("mismatch must fail");
        for needle in ["dead111", "beef222", running, "/usr/local/bin/x", "MISMATCH"] {
            assert!(err.contains(needle), "error names {needle}: {err}");
        }
        assert!(!err.contains('✅'), "no success glyph in a failure: {err}");

        // running core has no buildSha at all (pre-#194 binary still serving) → stale, loud
        let err = deploy_verdict(None, "beef222", "src", running).expect_err("no sha = stale");
        assert!(err.contains("MISMATCH") && err.contains("beef222"), "got {err}");

        // 'unknown' on either side is unverifiable — never a pass
        assert!(deploy_verdict(Some("unknown"), "beef222", "src", running).is_err());
        assert!(deploy_verdict(Some("dead111"), "unknown", "src", running).is_err());

        // sha_matches never matches junk (empty, non-hex, too short to be credible)
        assert!(!sha_matches("", ""));
        assert!(!sha_matches("unknown", "unknown"));
        assert!(!sha_matches("abc", "abc"));
    }

    // what this catches (#194): the artifact resolution ORDER is a shared contract with
    // tools/scripts/install-service.sh::resolve_core_bin — installed locations before the
    // cargo target dir (release before debug), default target dir under ~/.continuum/cache.
    // A drift here means the CLI verifies a different binary than the service installer
    // deploys — and NOTHING may resolve "next to the CLI exe" (the guess that printed
    // "could not locate continuum-core-server" while a 2-day-old core kept serving).
    #[test]
    fn artifact_resolution_order_matches_install_service() {
        let c = core_artifact_candidates("/home/u", None);
        let shown: Vec<String> = c.iter().map(|p| p.display().to_string()).collect();
        #[cfg(not(windows))]
        assert_eq!(
            shown,
            vec![
                "/usr/local/bin/continuum-core-server",
                "/home/u/.continuum/bin/continuum-core-server",
                "/home/u/.continuum/cache/cargo-target/release/continuum-core-server",
                "/home/u/.continuum/cache/cargo-target/debug/continuum-core-server",
            ],
            "order is the install-service.sh contract"
        );
        #[cfg(windows)]
        {
            assert!(shown.iter().all(|p| p.ends_with("continuum-core-server.exe")));
            assert!(
                !shown.iter().any(|p| p.starts_with("/usr/local/bin")),
                "no unix-only install location on Windows"
            );
        }
        // explicit CARGO_TARGET_DIR overrides the default cache location
        let c = core_artifact_candidates("/home/u", Some("/tgt"));
        assert!(
            c.iter().any(|p| p.starts_with("/tgt/release")) && c.iter().any(|p| p.starts_with("/tgt/debug")),
            "CARGO_TARGET_DIR is honored: {c:?}"
        );
    }
}

fn usage() -> String {
    "usage: continuum <start|reboot|stop|command> [json | --key value ...]\n\
     \n\
     Lifecycle:\n  \
       continuum start                 build + run the headless Rust core (detached), wait until ready\n  \
       continuum reboot                rebuild + relaunch, replacing any running core (~0 downtime);\n                                       verifies the RUNNING core's build SHA before reporting success\n  \
       continuum stop                  stop the running core\n  \
       continuum deploy-verify         prove the running core's build SHA matches the deployed source\n\
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
