//! Bounded execution and receipts for explicit prebuilt validation.

use serde::{Deserialize, Serialize};
use std::{
    process::{Command, Output, Stdio},
    time::Duration,
};

pub const EMBEDDING_FLAG: &str = "--validate-embedding-model";
pub const CAPABILITIES_FLAG: &str = "--validation-capabilities";
pub const EMBEDDING_DEADLINE: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub build_sha: String,
    pub embedding_probe: u32,
}

pub enum Probe<'a> {
    Capabilities,
    Embedding(&'a std::path::Path),
}

impl Probe<'_> {
    pub fn configure(&self, command: &mut Command) {
        match self {
            // Old candidates already exit safely for --build-sha. Their plain
            // SHA reply will fail parsing, before an unfamiliar flag is sent.
            Self::Capabilities => {
                command.args(["--build-sha", CAPABILITIES_FLAG]);
            }
            Self::Embedding(path) => {
                command.arg(EMBEDDING_FLAG).arg(path);
            }
        }
    }
}

pub async fn embedding(
    mut command: impl FnMut(Probe<'_>) -> Result<Command, String>,
    model: &std::path::Path,
    expected_sha: &str,
) -> Result<EmbeddingReport, String> {
    let output = run(command(Probe::Capabilities)?, Duration::from_secs(30)).await?;
    let capabilities: Capabilities = serde_json::from_slice(&output.stdout).map_err(|_| {
        "candidate does not advertise safe embedding validation; no model probe launched"
    })?;
    if capabilities.embedding_probe != 1
        || !crate::deploy_provenance::sha_matches(&capabilities.build_sha, expected_sha)
    {
        return Err(
            "candidate embedding capability/revision does not match the prepared artifact".into(),
        );
    }
    let output = run(command(Probe::Embedding(model))?, EMBEDDING_DEADLINE).await?;
    EmbeddingReport::parse(&output.stdout)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingReport {
    pub dimensions: usize,
    pub load_ms: u64,
    pub embed_ms: u64,
}

impl EmbeddingReport {
    pub fn validate(&self) -> Result<(), String> {
        if self.dimensions == 0 {
            return Err("embedding validation returned an empty vector".into());
        }
        Ok(())
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let report: Self = serde_json::from_slice(bytes)
            .map_err(|e| format!("invalid embedding validation receipt: {e}"))?;
        report.validate()?;
        Ok(report)
    }
}

/// Applies to provenance and model probes alike. Dropping a timed-out/cancelled
/// output future kills its owned child; Tokio also reaps that child. No service
/// process or descendant is started by either supported candidate probe.
pub async fn run(mut command: Command, deadline: Duration) -> Result<Output, String> {
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let output = tokio::time::timeout(deadline, command.output())
        .await
        .map_err(|_| {
            format!(
                "prebuilt validation exceeded {} seconds; child stopped",
                deadline.as_secs()
            )
        })?
        .map_err(|e| format!("cannot run prebuilt validation: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "prebuilt validation exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    // One subprocess fixture across OS adapters; no shell children survive a timeout.
    #[test]
    fn child_fixture() {
        let Ok(mode) = std::env::var("CONTINUUM_VALIDATION_TEST") else {
            return;
        };
        if mode == "old" {
            println!("abc123f0123456789");
        }
        if mode == "wait" {
            std::thread::sleep(Duration::from_millis(500));
            std::fs::write(
                std::env::var_os("CONTINUUM_VALIDATION_MARKER").unwrap(),
                "leaked",
            )
            .unwrap();
        }
        std::process::exit(if mode == "fail" { 7 } else { 0 });
    }

    #[tokio::test]
    async fn validation_rejects_failed_or_hung_candidates_and_invalid_receipts() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("child-completed");
        for mode in ["success", "fail", "wait"] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "prebuilt_validation::tests::child_fixture",
                    "--nocapture",
                ])
                .env("CONTINUUM_VALIDATION_TEST", mode)
                .env("CONTINUUM_VALIDATION_MARKER", &marker);
            let deadline = if mode == "wait" {
                Duration::from_millis(100)
            } else {
                Duration::from_secs(10)
            };
            assert_eq!(run(command, deadline).await.is_ok(), mode == "success");
        }
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(!marker.exists(), "timed-out child must not remain alive");
        let mut probes = 0;
        let refused = embedding(
            |probe| {
                assert!(
                    matches!(probe, Probe::Capabilities),
                    "old core must never receive a model probe"
                );
                let mut argv = Command::new("candidate");
                probe.configure(&mut argv);
                assert_eq!(
                    argv.get_args().collect::<Vec<_>>(),
                    ["--build-sha", "--validation-capabilities"]
                );
                probes += 1;
                let mut command = Command::new(std::env::current_exe().unwrap());
                command
                    .args([
                        "--exact",
                        "prebuilt_validation::tests::child_fixture",
                        "--nocapture",
                    ])
                    .env("CONTINUUM_VALIDATION_TEST", "old");
                Ok(command)
            },
            std::path::Path::new("unused.gguf"),
            "abc123f0123456789",
        )
        .await;
        assert!(refused.is_err());
        assert_eq!(probes, 1);
        assert!(EmbeddingReport::parse(br#"{"dimensions":32,"load_ms":1,"embed_ms":2}"#).is_ok());
        for invalid in [
            b"not a receipt".as_slice(),
            br#"{"dimensions":0,"load_ms":1,"embed_ms":2}"#,
            br#"{"dimensions":32}"#,
        ] {
            assert!(EmbeddingReport::parse(invalid).is_err());
        }
    }
}
