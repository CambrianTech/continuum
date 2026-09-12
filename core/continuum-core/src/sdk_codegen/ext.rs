//! Bring your own commands — a COMMAND IS A MANIFEST (Phase C, S7 of
//! docs/planning/RECIPE-CONVERGENCE-PLAN.md).
//!
//! Until this module, a recipe could only name verbs that are Rust in this repository.
//! The escape hatch existed (`code/shell` runs any process) but a user's script was not a
//! VERB: no name, no schema, not listable, not affordance-able, invisible to the recipe
//! gate. Now a JSON file in `<continuum_root>/commands/` declares one — name, description,
//! access, params schema, the argv to spawn — and it becomes indistinguishable from a
//! shipped verb to `commands/list`, `commands/help`, the persona's tool surface, the ACL,
//! the recipe validator, and `activity/spawn`. No deploy, no repo change.
//!
//! Two seams carry it, both pre-existing: the kernel's runtime command map
//! ([`crate::runtime::ModuleRegistry`] `command_objects`, fed by
//! [`crate::runtime::ServiceModule::commands`]) routes the call; and every catalogue
//! reader switches from the compile-time [`super::command_registry`] to
//! [`command_registry_live`], the same list plus the manifests' descriptors. The
//! compile-time registry itself is untouched — codegen still describes the SHIPPED
//! surface only, which is correct: a manifest is content, not source.
//!
//! Performance: one bounded spawn per call, JSON on stdin, JSON on stdout — never on a
//! turn's hot path unless a room's recipe offers the verb as an affordance, in which case
//! it is one act, priced like any other. `wire: "socket"` (a long-lived process, one
//! connection) is declared in the schema as the next tier and refused until built.

use super::{AccessLevel, CommandDescriptor, DynCommand, TypeRef, WireShape};
use crate::runtime::CommandResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Where an authored command's process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/commands/ManifestCwd.ts")]
#[serde(rename_all = "snake_case")]
pub enum ManifestCwd {
    /// The continuum root (`~/.continuum`). The default.
    #[default]
    Root,
    /// The calling citizen's workspace when known, else the root.
    Workspace,
}

/// How params reach the process and the result comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/commands/ManifestWire.ts")]
#[serde(rename_all = "snake_case")]
pub enum ManifestWire {
    /// Params as one JSON document on stdin; stdout is the JSON result. The default.
    #[default]
    JsonStdio,
    /// A long-lived process holding one connection. Declared, not yet built: a
    /// manifest naming it is refused at load with that reason.
    Socket,
}

/// The authored command. One file, one verb. Published as
/// `protocol/schema/command-manifest.schema.json` so an editor validates it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/commands/CommandManifest.ts")]
#[serde(rename_all = "camelCase")]
pub struct CommandManifest {
    /// The verb, e.g. `ext/proof-check`. Must not collide with a shipped command.
    pub name: String,
    /// One line, model-facing: what the persona's tool surface shows the reasoner.
    #[serde(default)]
    pub description: String,
    /// `ai_safe` (a citizen may call it; the ACL maps it to Provisional trust) or
    /// `privileged` (operator and substrate only).
    #[serde(default)]
    pub access: ManifestAccess,
    /// Opt into the persona's NATIVE tool surface (offered as a tool schema, not
    /// merely reachable by name). Keep the native set small; see `native_tool_specs`.
    #[serde(default)]
    pub native: bool,
    /// JSON Schema for the params object. The recipe gate validates steps against it
    /// and `commands/help` renders it — declare it, or the verb is untyped.
    #[serde(default)]
    #[ts(type = "unknown")]
    pub params: Value,
    /// argv. The first element resolves on PATH or as a path relative to the manifest.
    pub exec: Vec<String>,
    #[serde(default)]
    pub cwd: ManifestCwd,
    #[serde(default)]
    pub wire: ManifestWire,
    /// Every process call is bounded and named. Default 60 000 ms.
    #[serde(default = "default_timeout_ms")]
    #[ts(type = "number")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    60_000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/commands/ManifestAccess.ts")]
#[serde(rename_all = "snake_case")]
pub enum ManifestAccess {
    #[default]
    AiSafe,
    Privileged,
}

impl From<ManifestAccess> for AccessLevel {
    fn from(a: ManifestAccess) -> Self {
        match a {
            ManifestAccess::AiSafe => AccessLevel::AiSafe,
            ManifestAccess::Privileged => AccessLevel::Privileged,
        }
    }
}

/// Why a manifest was refused. Every variant names the file and the reason; a
/// refused manifest never loads silently, and never refuses the BOOT.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{path}: cannot read: {source}")]
    Read { path: PathBuf, #[source] source: std::io::Error },
    #[error("{path}: malformed manifest: {source}")]
    Parse { path: PathBuf, #[source] source: serde_json::Error },
    #[error("{path}: `{name}` collides with a shipped command — authored verbs never shadow built-ins")]
    Collision { path: PathBuf, name: String },
    #[error("{path}: `{name}` is declared twice in this directory")]
    Duplicate { path: PathBuf, name: String },
    #[error("{path}: `exec` is empty — a command must name a program")]
    NoExec { path: PathBuf },
    #[error("{path}: wire `socket` is declared but not built yet — use json_stdio")]
    WireNotBuilt { path: PathBuf },
    #[error("{path}: `{name}` is not a verb name (segments of [a-z0-9_-], joined by /)")]
    BadName { path: PathBuf, name: String },
}

/// The overlay directory for authored commands: `<continuum_root>/commands`, the same
/// law as recipes (`RecipeExperienceSource::overlay_dir`).
pub fn overlay_dir(continuum_root: &Path) -> PathBuf {
    continuum_root.join("commands")
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('/').all(|seg| {
            !seg.is_empty() && seg.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        })
}

/// One loaded manifest: the routable object and its catalogue descriptor.
pub struct LoadedCommand {
    pub command: Arc<ProcessCommand>,
    pub descriptor: CommandDescriptor,
}

/// Read every `*.json` manifest in `dir` (sorted, so two nodes agree), refusing each
/// bad one by file and reason. A missing directory is the ordinary state (no local
/// authoring yet) and yields nothing. `shipped` is the set of built-in names a
/// manifest may not shadow.
pub fn load_manifests(dir: &Path, shipped: &[&str]) -> (Vec<LoadedCommand>, Vec<ManifestError>) {
    let mut loaded = Vec::new();
    let mut refused = Vec::new();
    if !dir.is_dir() {
        return (loaded, refused);
    }
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect(),
        Err(source) => {
            refused.push(ManifestError::Read { path: dir.to_path_buf(), source });
            return (loaded, refused);
        }
    };
    paths.sort();
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        match load_one(&path, shipped) {
            Ok(cmd) => {
                if !seen.insert(cmd.manifest.name.clone()) {
                    refused.push(ManifestError::Duplicate { path, name: cmd.manifest.name.clone() });
                    continue;
                }
                let descriptor = cmd.descriptor();
                loaded.push(LoadedCommand { command: Arc::new(cmd), descriptor });
            }
            Err(e) => refused.push(e),
        }
    }
    (loaded, refused)
}

fn load_one(path: &Path, shipped: &[&str]) -> Result<ProcessCommand, ManifestError> {
    let text = std::fs::read_to_string(path).map_err(|source| ManifestError::Read { path: path.to_path_buf(), source })?;
    let manifest: CommandManifest =
        serde_json::from_str(&text).map_err(|source| ManifestError::Parse { path: path.to_path_buf(), source })?;
    if !valid_name(&manifest.name) {
        return Err(ManifestError::BadName { path: path.to_path_buf(), name: manifest.name });
    }
    if shipped.contains(&manifest.name.as_str()) {
        return Err(ManifestError::Collision { path: path.to_path_buf(), name: manifest.name });
    }
    if manifest.exec.is_empty() {
        return Err(ManifestError::NoExec { path: path.to_path_buf() });
    }
    if manifest.wire == ManifestWire::Socket {
        return Err(ManifestError::WireNotBuilt { path: path.to_path_buf() });
    }
    Ok(ProcessCommand::new(manifest, path.parent().map(Path::to_path_buf).unwrap_or_default())) // unwrap_or_default: a manifest read from a bare filename runs in the process cwd
}

/// A verb backed by a process. `name`/`description` are leaked ONCE at load — the
/// registry keys on `&'static str`, and a manifest set is bounded and loaded once
/// per process, so this is a boot-time allocation, not a leak per call.
pub struct ProcessCommand {
    manifest: CommandManifest,
    name: &'static str,
    description: &'static str,
    manifest_dir: PathBuf,
}

impl ProcessCommand {
    pub fn new(manifest: CommandManifest, manifest_dir: PathBuf) -> Self {
        let name: &'static str = Box::leak(manifest.name.clone().into_boxed_str());
        let description: &'static str = Box::leak(manifest.description.clone().into_boxed_str());
        Self { manifest, name, description, manifest_dir }
    }

    pub fn manifest(&self) -> &CommandManifest {
        &self.manifest
    }

    /// The program to run: a bare name resolves on PATH; a relative path resolves
    /// beside the manifest, so a shared directory carries its scripts with it.
    fn program(&self) -> PathBuf {
        let first = &self.manifest.exec[0];
        let p = Path::new(first);
        if p.components().count() > 1 && p.is_relative() {
            self.manifest_dir.join(p)
        } else {
            p.to_path_buf()
        }
    }

    fn working_dir(&self) -> PathBuf {
        match self.manifest.cwd {
            ManifestCwd::Root | ManifestCwd::Workspace => {
                crate::modules::persona_instance_manager::resolve_continuum_root()
            }
        }
    }

    /// Run the process once: params on stdin, result from stdout, bounded.
    pub async fn run_process(&self, params: Value) -> Result<Value, String> {
        use tokio::io::AsyncWriteExt;
        let started = std::time::Instant::now();
        let mut cmd = tokio::process::Command::new(self.program());
        cmd.args(&self.manifest.exec[1..])
            .current_dir(self.working_dir())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("{}: cannot spawn {:?}: {e}", self.name, self.manifest.exec[0]))?;
        crate::probe!(
            class = "ext.command.spawned",
            command = %self.name,
            program = %self.manifest.exec[0],
            "authored command process started"
        );
        if let Some(mut stdin) = child.stdin.take() {
            let body = serde_json::to_vec(&params).map_err(|e| e.to_string())?; // boundary: params leave this process on the child's stdin
            // A program that never reads stdin must not wedge the call: write, then drop.
            let _ = stdin.write_all(&body).await;
            let _ = stdin.shutdown().await;
        }
        let bound = std::time::Duration::from_millis(self.manifest.timeout_ms);
        let out = match tokio::time::timeout(bound, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(format!("{}: process failed: {e}", self.name)),
            Err(_) => {
                crate::probe!(
                    class = "ext.command.timed_out",
                    command = %self.name,
                    timeout_ms = self.manifest.timeout_ms,
                    "authored command exceeded its declared bound — killed"
                );
                return Err(format!("{}: timed out after {} ms", self.name, self.manifest.timeout_ms));
            }
        };
        let elapsed_ms = started.elapsed().as_millis() as u64;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            crate::probe!(
                class = "ext.command.failed",
                command = %self.name,
                code = out.status.code().unwrap_or(-1), // unwrap_or: killed by a signal has no exit code; -1 names that // unwrap_or: killed by a signal has no exit code; -1 names that
                elapsed_ms,
                "authored command exited non-zero"
            );
            return Err(format!(
                "{}: exit {} — {}",
                self.name,
                out.status.code().unwrap_or(-1),
                stderr.trim().chars().take(600).collect::<String>()
            ));
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let value = if stdout.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(stdout.trim()).map_err(|e| {
                format!("{}: stdout is not JSON ({e}): {}", self.name, stdout.trim().chars().take(200).collect::<String>())
            })?
        };
        crate::probe!(
            class = "ext.command.finished",
            command = %self.name,
            elapsed_ms,
            "authored command returned"
        );
        Ok(value)
    }
}

#[async_trait]
impl DynCommand for ProcessCommand {
    fn name(&self) -> &'static str {
        self.name
    }

    fn descriptor(&self) -> CommandDescriptor {
        // Runtime-only: this descriptor feeds the catalogue, the tool surface, the ACL and
        // the recipe gate. It never enters codegen (the compile-time registry is
        // untouched), so the TypeRefs are a stable placeholder, not a TS type.
        let json = TypeRef { name: "JsonValue".to_string(), module: "ext".to_string() };
        CommandDescriptor {
            name: self.name,
            access_level: self.manifest.access.into(),
            description: self.description,
            wire: WireShape::Bare,
            native: self.manifest.native,
            aliases: &[],
            params: json.clone(),
            params_schema: self.manifest.params.clone(),
            result: json.clone(),
            type_refs: vec![json],
        }
    }

    async fn invoke(
        &self,
        params: Value,
        _caller: Option<crate::routing::CallerIdentity>,
    ) -> Result<CommandResult, String> {
        self.run_process(params).await.map(CommandResult::Json)
    }
}

static EXT_DESCRIPTORS: std::sync::OnceLock<Vec<CommandDescriptor>> = std::sync::OnceLock::new();

/// Install the manifests' descriptors for this process. Called ONCE at boot, before
/// any catalogue reader memoizes (the ACL's command sets, the tool dialect index).
/// A second install is a boot-order bug, reported and ignored.
pub fn install_ext_descriptors(descriptors: Vec<CommandDescriptor>) {
    if EXT_DESCRIPTORS.set(descriptors).is_err() {
        tracing::error!(target: "ext", "authored command descriptors installed twice — the first install stands");
    }
}

/// THE ONE UNION POINT: every command this node can route — the compile-time registry
/// plus every authored manifest — sorted by name. Every catalogue reader uses this;
/// codegen alone keeps reading [`super::command_registry`].
pub fn command_registry_live() -> Vec<CommandDescriptor> {
    let mut all = super::command_registry();
    if let Some(ext) = EXT_DESCRIPTORS.get() {
        all.extend(ext.iter().cloned());
        all.sort_by(|a, b| a.name.cmp(b.name));
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(name: &str, exec: &[&str]) -> CommandManifest {
        CommandManifest {
            name: name.into(),
            description: "t".into(),
            access: ManifestAccess::AiSafe,
            native: false,
            params: serde_json::json!({ "type": "object", "properties": { "x": { "type": "string" } } }),
            exec: exec.iter().map(|s| s.to_string()).collect(),
            cwd: ManifestCwd::Root,
            wire: ManifestWire::JsonStdio,
            timeout_ms: 5_000,
        }
    }

    // what this catches: a manifest is refused by FILE and REASON for each way it can be
    // wrong — a shadowed built-in, a duplicate, an empty exec, an unbuilt wire, a bad name
    // — and a good one loads with the schema it declared. A silent skip here is an
    // author who believes a verb is live when it is not.
    #[test]
    fn manifests_load_or_are_refused_by_file_and_reason() {
        let dir = tempfile::tempdir().expect("tempdir");
        let w = |f: &str, m: &CommandManifest| std::fs::write(dir.path().join(f), serde_json::to_string(m).unwrap()).unwrap(); // boundary: the test writes a manifest FILE the loader reads from disk
        w("a.json", &manifest("ext/good", &["cat"]));
        w("b.json", &manifest("ping", &["cat"]));                 // collides with a shipped verb
        w("c.json", &manifest("ext/good", &["cat"]));            // duplicate of a.json
        w("d.json", &manifest("ext/noexec", &[]));               // nothing to run
        let mut sock = manifest("ext/sock", &["cat"]); sock.wire = ManifestWire::Socket; w("e.json", &sock);
        w("f.json", &manifest("Bad Name", &["cat"]));            // not a verb name
        std::fs::write(dir.path().join("g.json"), "{ not json").unwrap();

        let (loaded, refused) = load_manifests(dir.path(), &["ping"]);
        assert_eq!(loaded.len(), 1, "{refused:?}");
        assert_eq!(loaded[0].descriptor.name, "ext/good");
        assert_eq!(loaded[0].descriptor.params_schema["properties"]["x"]["type"], "string");
        let reasons: Vec<String> = refused.iter().map(ToString::to_string).collect();
        assert_eq!(refused.len(), 6, "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("collides") && r.contains("b.json")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("declared twice") && r.contains("c.json")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("exec") && r.contains("d.json")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("socket") && r.contains("e.json")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("not a verb name") && r.contains("f.json")), "{reasons:?}");
        assert!(reasons.iter().any(|r| r.contains("malformed") && r.contains("g.json")), "{reasons:?}");
    }

    // what this catches: the wire. Params go in as JSON on stdin, the result comes back
    // as JSON on stdout, and a non-zero exit is a named error — the whole contract an
    // author's script is written against. `cat` is the identity command.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_command_round_trips_json_and_names_a_failure() {
        let ok = ProcessCommand::new(manifest("ext/echo", &["cat"]), PathBuf::new());
        let v = ok.run_process(serde_json::json!({ "x": "hello", "n": 3 })).await.expect("cat echoes");
        assert_eq!(v["x"], "hello");
        assert_eq!(v["n"], 3);

        let bad = ProcessCommand::new(manifest("ext/fail", &["sh", "-c", "echo boom >&2; exit 3"]), PathBuf::new());
        let err = bad.run_process(Value::Null).await.expect_err("non-zero exit is an error");
        assert!(err.contains("exit 3") && err.contains("boom"), "{err}");
    }

    // what this catches: the bound. A process that never returns is killed at its
    // declared timeout and the error names it — never a wedged turn.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_process_that_hangs_is_killed_at_its_declared_bound() {
        let mut m = manifest("ext/hang", &["sleep", "30"]);
        m.timeout_ms = 200;
        let hang = ProcessCommand::new(m, PathBuf::new());
        let err = hang.run_process(Value::Null).await.expect_err("bounded");
        assert!(err.contains("timed out after 200 ms"), "{err}");
    }

    // what this catches: the manifest file has a PUBLISHED schema, like the recipe —
    // an author's editor validates it, and the drift job reviews changes to it.
    #[test]
    fn the_manifest_schema_is_published_beside_the_bindings() {
        let mut schema = schemars::schema_for!(CommandManifest);
        schema.schema.metadata().title = Some("Continuum command manifest".to_string());
        schema.schema.metadata().description = Some(
            "An authored command: a verb backed by a process on this machine. Drop the file in \
             <continuum_root>/commands/ — it is live on the next boot, no deploy, no repo change."
                .to_string(),
        );
        let json = serde_json::to_string_pretty(&schema).expect("schema serializes") + "\n"; // boundary: the published schema file on disk
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../protocol/schema/command-manifest.schema.json");
        std::fs::create_dir_all(path.parent().expect("schema dir")).expect("mkdir");
        std::fs::write(&path, json).expect("write the published schema");
    }
}
