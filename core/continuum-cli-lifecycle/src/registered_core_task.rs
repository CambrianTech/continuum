//! Registered core task descriptor and validation, shared by lifecycle callers.
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("ContinuumCore path must be absolute: {0}")]
    Relative(PathBuf),
    #[error("ContinuumCore path {path} cannot be resolved: {source}")]
    Resolve { path: PathBuf, #[source] source: std::io::Error },
    #[error("installed release changed during installer-to-reboot transfer; refusing teardown")]
    Changed,
    #[error("invalid installer artifact descriptor: {0}")]
    Descriptor(#[from] serde_json::Error),
    #[error("ContinuumCore logDirectory is not a directory")]
    LogDirectory,
    #[error("ContinuumCore engine is not an installed file")]
    EngineFile,
    #[error(transparent)]
    InstallerRoot(#[from] crate::installed_release::InstallerRootError),
    #[error("ContinuumCore's scheduled task is disabled")]
    Disabled,
    #[error("ContinuumCore selects artifact {selected} but the requested core is {requested}")]
    Artifact { selected: PathBuf, requested: PathBuf },
    #[error("ContinuumCore selects socket {selected} but the requested socket is {requested}")]
    Socket { selected: PathBuf, requested: PathBuf },
    #[error("ContinuumCore task runs {command} but this host's shell is {shell}")]
    Shell { command: PathBuf, shell: PathBuf },
    #[error("ContinuumCore host/launcher must be files beside the selected artifact")]
    PairedFiles,
    #[error("ContinuumCore requires the installed CLI and hidden launcher")]
    LauncherNames,
    #[error("ContinuumCore descriptor cannot be quoted as a task argument")]
    ArgumentQuoting,
    #[error("ContinuumCore eyeRoot must be an absolute, quotable asset directory")]
    EyeRoot,
    #[error("ContinuumCore stable bootstrap action differs")]
    BootstrapAction,
    #[error("ContinuumCore action differs from its artifact descriptor")]
    Action,
}

#[derive(Debug, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CoreServiceTask {
    pub description: String,
    pub command: String,
    pub arguments: String,
    pub enabled: bool,
    pub state: String,
    #[serde(default)]
    pub working_directory: String,
    #[serde(skip)]
    pub installed: Option<crate::installed_release::Envelope>,
}

/// A path the ContinuumCore descriptor or the installer's receipt names, absolute and resolved,
/// or why not. THE one resolver for both sides of [`CoreServiceTask::validate`]'s comparison:
/// a receipt holds an ordinary `C:\...` path while a resolved one carries Windows' verbatim
/// `\\?\` prefix, so a receipt compared raw refused every unattended deploy on the 5090
/// (ce84f63ac, 2026-10-12 19:31Z; Codex traced it).
pub fn resolve_service_path(path: impl AsRef<Path>) -> Result<PathBuf, ValidationError> {
    let path = path.as_ref();
    if !path.is_absolute() {
        return Err(ValidationError::Relative(path.to_path_buf()));
    }
    std::fs::canonicalize(path).map_err(|source| ValidationError::Resolve { path: path.to_path_buf(), source })
}

impl CoreServiceTask {
    pub fn validate_description_sha(&self, expected: &str) -> Result<(), ValidationError> {
        use sha2::{Digest, Sha256};
        let actual = format!("{:x}", Sha256::digest(self.description.as_bytes()));
        if actual != expected {
            return Err(ValidationError::Changed);
        }
        Ok(())
    }

    /// What a restart needs from the registered release that staging does not write: a
    /// parseable descriptor whose engine is an installed file and whose log directory
    /// exists. The reboot checks this BEFORE it stops the running core, and `validate`
    /// re-checks it after staging. Measured on the 5090 2026-10-06: the descriptor named an
    /// engine slot that had been moved aside, `validate` first looked after the stop, and the
    /// node sat with no core until a hand restored the slot. A deploy that cannot start the
    /// next core must refuse while the old one still serves.
    pub fn runtime_paths_resolve(&self) -> Result<crate::installed_release::Release, ValidationError> {
        let description: crate::installed_release::Release =
            serde_json::from_str(&self.description)?;
        if !resolve_service_path(&description.log_directory)?.is_dir() {
            return Err(ValidationError::LogDirectory);
        }
        if !resolve_service_path(&description.engine)?.is_file() {
            return Err(ValidationError::EngineFile);
        }
        Ok(description)
    }

    pub fn validate(&self, candidate: &Path, socket: &str, shell: &Path) -> Result<(), ValidationError> {
        let description = self.runtime_paths_resolve()?;
        if self.installed.is_some() { description.installer_root()?; }
        let resolve = resolve_service_path;
        // Each refusal names its condition and both values: one message for four conditions
        // left the 5090's deploy log unable to say which held (card f5fd64ad).
        if !self.enabled {
            return Err(ValidationError::Disabled);
        }
        let selected = resolve(&description.artifact)?;
        if selected != candidate {
            return Err(ValidationError::Artifact { selected, requested: candidate.to_path_buf() });
        }
        if description.socket != socket {
            return Err(ValidationError::Socket { selected: description.socket.into(), requested: socket.into() });
        }
        if self.installed.is_none() {
            let command = resolve(&self.command)?;
            let shell = resolve_service_path(shell)?;
            if command != shell {
                return Err(ValidationError::Shell { command, shell });
            }
        }
        // Paired host/launcher files live in the same installed slot, never in
        // the mutable build cache or a different release's directory.
        for path in [&description.cli, &description.launcher] {
            let path = resolve(path)?;
            if !path.is_file() || path.parent() != candidate.parent() {
                return Err(ValidationError::PairedFiles);
            }
        }
        if Path::new(&description.cli).file_name() != Some(std::ffi::OsStr::new("continuum.exe"))
            || Path::new(&description.launcher).file_name()
                != Some(std::ffi::OsStr::new("run-service-hidden.ps1"))
        {
            return Err(ValidationError::LauncherNames);
        }
        for value in [
            &description.artifact,
            &description.socket,
            &description.launcher,
            &description.cli,
            &description.engine,
            &description.log_directory,
        ] {
            if value.contains(['"', '\r', '\n']) || value.ends_with('\\') {
                return Err(ValidationError::ArgumentQuoting);
            }
        }
        let mut expected = format!(
            "-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy RemoteSigned -File \"{}\" -ExecutablePath \"{}\" -CorePath \"{}\" -SocketPath \"{}\" -EnginePath \"{}\" -LogDirectory \"{}\"",
            description.launcher, description.cli, description.artifact,
            description.socket, description.engine, description.log_directory,
        );
        if let Some(root) = &description.eye_root {
            if !Path::new(root).is_absolute() || root.contains(['"', '\r', '\n']) || root.ends_with('\\') {
                return Err(ValidationError::EyeRoot);
            }
            expected.push_str(&format!(" -EyeRoot \"{root}\""));
        }
        if let Some(envelope) = &self.installed {
            if resolve(&self.command)? != resolve(&envelope.bootstrap)? || self.arguments != envelope.arguments("core") {
                return Err(ValidationError::BootstrapAction);
            }
        } else if self.arguments != expected {
            return Err(ValidationError::Action);
        }
        Ok(())
    }
}

#[cfg(windows)]
pub async fn query_optional(home: &Path, payload_root: impl FnOnce(&Path) -> Result<PathBuf, String>) -> Result<Option<CoreServiceTask>, String> {
        let json = query_shell(
            "$t=Get-ScheduledTask | Where-Object { $_.TaskName -eq 'ContinuumCore' -and $_.TaskPath -eq '\\' }; if ($null -eq $t) { 'null'; exit 0 }; if (@($t.Actions).Count -ne 1) { throw 'Expected one core action' }; [pscustomobject]@{description=$t.Description; command=$t.Actions[0].Execute; arguments=$t.Actions[0].Arguments; workingDirectory=$t.Actions[0].WorkingDirectory; enabled=[bool]$t.Settings.Enabled; state=[string]$t.State} | ConvertTo-Json -Compress",
        ).await.map_err(|error| error.to_string())?;
        let mut task: Option<CoreServiceTask> = serde_json::from_str(&json).map_err(|e| format!("cannot read ContinuumCore task: {e}"))?;
        if let Some(task) = &mut task {
            if let Some(envelope) = crate::installed_release::envelope(&task.description).map_err(|error| error.to_string())? {
                let sid = crate::supervisor_install::caller_sid().await?;
                let program_files = PathBuf::from(std::env::var_os("ProgramFiles").ok_or("ProgramFiles is unset")?);
                envelope.validate(&home, &program_files, &sid)?;
                if task.command.to_lowercase() != envelope.bootstrap.to_lowercase() || task.arguments != envelope.arguments("core") || Path::new(&task.working_directory) != Path::new(&envelope.bootstrap).parent().ok_or("bootstrap has no directory")? {
                    return Err("ContinuumCore stable task action differs from its envelope".into());
                }
                let text = crate::installed_release::read_receipt(Path::new(&envelope.active_release))?;
                let payload = payload_root(home)?;
                let _resolved = crate::installed_release::resolve_receipt(&text,&home,&payload,&sid)?;
                // Preserve PowerShell's descriptor field ordering for the existing SHA handoff.
                // This serializes the SAME validated bytes, never a second racy file read.
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes()); // Active receipt bytes cross the PowerShell process boundary without argument quoting ambiguity.
                task.description = query_shell(&format!("$r=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}')) | ConvertFrom-Json; $r.release | ConvertTo-Json -Compress")).await.map_err(|error| error.to_string())?;
                task.installed = Some(envelope);
            }
        }
        Ok(task)
    }

#[derive(Debug, thiserror::Error)]
enum QueryOutputError {
    #[error("ContinuumCore query: {0}")]
    Process(#[from] continuum_host::powershell::PowerShellError),
    #[error("ContinuumCore query returned invalid UTF-8: {0}")]
    Encoding(#[from] std::string::FromUtf8Error),
}

fn decode_query_output(stdout: Vec<u8>) -> Result<String, QueryOutputError> {
    // These bytes become the descriptor used for the teardown SHA handoff. Lossy
    // decoding could silently change a path or identity before validation.
    Ok(String::from_utf8(stdout)?.trim().to_owned())
}

#[cfg(windows)]
async fn query_shell(script: &str) -> Result<String, QueryOutputError> {
    let protocol = format!("$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); {script}");
    let output = continuum_host::powershell::run(&protocol, std::time::Duration::from_secs(30)).await?;
    decode_query_output(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_registered_envelopes_keep_their_failure_kind() {
        use crate::installed_release::{envelope, EnvelopeError};
        assert!(matches!(envelope("{"), Err(EnvelopeError::Descriptor(_))));
        assert!(matches!(envelope(r#"{"schema":2,"activeRelease":"missing"}"#), Err(EnvelopeError::Structure(_))));
        assert!(matches!(envelope(r#"{"schema":3,"activeRelease":"receipt","bootstrap":"cli"}"#), Err(EnvelopeError::Schema { actual: 3 })));
    }

    #[test]
    fn query_descriptor_bytes_are_never_lossily_repaired() -> Result<(), Box<dyn std::error::Error>> {
        let descriptor = "{\"engine\":\"C:/minds/雪/engine.exe\"}";
        assert_eq!(decode_query_output(format!("{descriptor}\r\n").into_bytes())?, descriptor);
        let invalid = [b'{', b'"', 0xff, b'"', b'}'];
        match decode_query_output(invalid.to_vec()) {
            Err(QueryOutputError::Encoding(error)) => assert_eq!(error.into_bytes(), invalid),
            other => return Err(format!("expected retained invalid bytes, got {other:?}").into()),
        }
        Ok(())
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn native_query_preserves_unicode_and_refuses_bad_bytes_and_failed_processes() -> Result<(), Box<dyn std::error::Error>> {
        let output = query_shell(
            "[pscustomobject]@{engine=[string][char]0x96ea} | ConvertTo-Json -Compress"
        ).await?;
        assert_eq!(output, "{\"engine\":\"雪\"}");

        let invalid = query_shell(
            "$s=[Console]::OpenStandardOutput(); $s.Write([byte[]]@(255),0,1); $s.Flush()"
        ).await;
        match invalid {
            Err(QueryOutputError::Encoding(error)) => assert_eq!(error.into_bytes(), vec![255]),
            other => return Err(format!("expected native invalid-byte refusal, got {other:?}").into()),
        }

        let failed = query_shell("[Console]::Error.Write('fixture-refusal'); exit 7").await;
        match failed {
            Err(QueryOutputError::Process(continuum_host::powershell::PowerShellError::Exit { status, stderr })) => {
                assert_eq!(status.code(), Some(7));
                assert_eq!(stderr, b"fixture-refusal");
            }
            other => return Err(format!("expected native process refusal, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn vanished_runtime_and_changed_descriptor_refuse_before_teardown() -> Result<(), Box<dyn std::error::Error>> {
        use sha2::{Digest, Sha256};
        let directory = tempfile::tempdir()?;
        let engine = directory.path().join("engine.exe");
        std::fs::write(&engine, b"fixture")?;
        let mut task = CoreServiceTask {
            description: serde_json::json!({
                "artifact": directory.path().join("core.exe"),
                "socket": "fixture.sock", "launcher": "launcher.ps1", "cli": "continuum.exe",
                "engine": engine, "logDirectory": directory.path(),
            }).to_string(),
            command: String::new(), arguments: String::new(), enabled: true,
            state: "Ready".into(), working_directory: String::new(), installed: None,
        };
        let expected = format!("{:x}", Sha256::digest(task.description.as_bytes()));
        task.validate_description_sha(&expected)?;
        task.runtime_paths_resolve()?;
        std::fs::remove_file(&engine)?;
        assert!(matches!(task.runtime_paths_resolve(), Err(ValidationError::Resolve { path, source })
            if path == engine && source.kind() == std::io::ErrorKind::NotFound));
        task.description.push(' ');
        assert!(matches!(task.validate_description_sha(&expected), Err(ValidationError::Changed)));
        Ok(())
    }
}
