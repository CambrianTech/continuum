use crate::vdd::artifacts::{ArtifactBundle, ArtifactWriter, ReproducibilityManifest};
use crate::vdd::record::{HarnessStatus, StandardVddRecord, VddError};
use async_trait::async_trait;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ChatRoundtripConfigError {
    #[error("CONTINUUM_CHAT_ROUNDTRIP_EXPECTED must be an unsigned integer: {0}")]
    InvalidExpectedResponses(std::num::ParseIntError),
    #[error("CONTINUUM_CHAT_ROUNDTRIP_EXPECTED must be valid unicode")]
    NonUnicodeExpectedResponses,
}

#[derive(Debug, Clone)]
pub struct ChatRoundtripConfig {
    pub expected_responses: u32,
    pub git_sha: String,
    pub command: String,
    pub socket_path: Option<PathBuf>,
    pub timeout: Duration,
}

impl ChatRoundtripConfig {
    pub fn from_env() -> Result<Self, ChatRoundtripConfigError> {
        let expected_responses = match std::env::var("CONTINUUM_CHAT_ROUNDTRIP_EXPECTED") {
            Ok(raw) => raw
                .parse::<u32>()
                .map_err(ChatRoundtripConfigError::InvalidExpectedResponses)?,
            Err(std::env::VarError::NotPresent) => 1,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(ChatRoundtripConfigError::NonUnicodeExpectedResponses);
            }
        };
        let git_sha = std::env::var("CONTINUUM_GIT_SHA").unwrap_or_else(|_| "unknown".to_string());
        let command = "cargo continuum-vdd chat-roundtrip-live".to_string();
        let socket_path = std::env::var_os("CONTINUUM_CHAT_ROUNDTRIP_SOCKET").map(PathBuf::from);
        Ok(Self {
            expected_responses,
            git_sha,
            command,
            socket_path,
            timeout: Duration::from_secs(30),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRoundtripObservation {
    pub first_response_ms: u64,
    pub all_responses_ms: u64,
    pub responses_observed: u32,
    pub silence_reasons: Vec<String>,
    pub log_refs: Vec<String>,
}

#[async_trait]
pub trait ChatRoundtripProbe {
    async fn observe(
        &self,
        config: &ChatRoundtripConfig,
    ) -> Result<ChatRoundtripObservation, ChatRoundtripProbeError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ChatRoundtripProbeError {
    #[error("missing live chat substrate prerequisite: {0}")]
    PrerequisiteMissing(String),
    #[error("chat roundtrip failed: {0}")]
    Failed(String),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LiveChatProbe;

#[async_trait]
impl ChatRoundtripProbe for LiveChatProbe {
    async fn observe(
        &self,
        config: &ChatRoundtripConfig,
    ) -> Result<ChatRoundtripObservation, ChatRoundtripProbeError> {
        let socket_path = config.socket_path.as_ref().ok_or_else(|| {
            ChatRoundtripProbeError::PrerequisiteMissing(
                "CONTINUUM_CHAT_ROUNDTRIP_SOCKET is not set".to_string(),
            )
        })?;
        if !socket_path.exists() {
            return Err(ChatRoundtripProbeError::PrerequisiteMissing(format!(
                "chat roundtrip socket does not exist: {}",
                socket_path.display()
            )));
        }
        Err(ChatRoundtripProbeError::PrerequisiteMissing(
            "live chat socket protocol adapter is not wired yet; refusing fake success".to_string(),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ChatRoundtripHarness<P> {
    probe: P,
    artifacts: ArtifactWriter,
}

impl<P> ChatRoundtripHarness<P> {
    pub fn new(probe: P, artifacts: ArtifactWriter) -> Self {
        Self { probe, artifacts }
    }
}

impl<P> ChatRoundtripHarness<P>
where
    P: ChatRoundtripProbe + Sync,
{
    pub async fn run(&self, config: ChatRoundtripConfig) -> Result<ArtifactBundle, VddError> {
        let record = self.measure(config).await;
        let manifest = ReproducibilityManifest::from_record(
            &record,
            &[
                "CONTINUUM_CHAT_ROUNDTRIP_SOCKET",
                "CONTINUUM_CHAT_ROUNDTRIP_EXPECTED",
                "CONTINUUM_HARNESS_HARDWARE_CLASS",
                "CONTINUUM_HARNESS_BACKEND",
            ],
        );
        self.artifacts.write(&record, &manifest)
    }

    pub async fn measure(&self, config: ChatRoundtripConfig) -> StandardVddRecord {
        let mut record = StandardVddRecord::chat_roundtrip(
            config.git_sha.clone(),
            config.command.clone(),
            config.expected_responses,
        );
        match self.probe.observe(&config).await {
            Ok(observation) => {
                record.first_response_ms = Some(observation.first_response_ms);
                record.all_responses_ms = Some(observation.all_responses_ms);
                record.responses_observed = observation.responses_observed;
                record.silence_reasons = observation.silence_reasons;
                record.log_refs = observation.log_refs;
                record.status = if record.responses_observed >= record.responses_expected
                    && record.silence_reasons.is_empty()
                {
                    HarnessStatus::Pass
                } else {
                    record.error_count = 1;
                    record.next_bottleneck =
                        Some("persona cognition did not emit the expected replies".to_string());
                    HarnessStatus::Fail
                };
            }
            Err(ChatRoundtripProbeError::PrerequisiteMissing(reason)) => {
                record.status = HarnessStatus::PrerequisiteMissing;
                record.degraded_reason = Some(reason);
                record.next_bottleneck =
                    Some("wire the real chat roundtrip substrate probe".into());
            }
            Err(ChatRoundtripProbeError::Failed(reason)) => {
                record.status = HarnessStatus::Fail;
                record.error_count = 1;
                record.degraded_reason = Some(reason);
            }
        }
        record
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vdd::record::HarnessStatus;
    use tempfile::tempdir;

    struct StaticProbe(Result<ChatRoundtripObservation, ChatRoundtripProbeError>);

    #[async_trait]
    impl ChatRoundtripProbe for StaticProbe {
        async fn observe(
            &self,
            _config: &ChatRoundtripConfig,
        ) -> Result<ChatRoundtripObservation, ChatRoundtripProbeError> {
            match &self.0 {
                Ok(observation) => Ok(observation.clone()),
                Err(ChatRoundtripProbeError::PrerequisiteMissing(reason)) => {
                    Err(ChatRoundtripProbeError::PrerequisiteMissing(reason.clone()))
                }
                Err(ChatRoundtripProbeError::Failed(reason)) => {
                    Err(ChatRoundtripProbeError::Failed(reason.clone()))
                }
            }
        }
    }

    fn config() -> ChatRoundtripConfig {
        ChatRoundtripConfig {
            expected_responses: 2,
            git_sha: "test-sha".to_string(),
            command: "cargo continuum-vdd chat-roundtrip-live".to_string(),
            socket_path: None,
            timeout: Duration::from_millis(10),
        }
    }

    #[tokio::test]
    async fn missing_live_substrate_is_not_a_pass() {
        let harness = ChatRoundtripHarness::new(
            StaticProbe(Err(ChatRoundtripProbeError::PrerequisiteMissing(
                "socket missing".to_string(),
            ))),
            ArtifactWriter::new(tempdir().unwrap().path()),
        );

        let record = harness.measure(config()).await;

        assert_eq!(record.status, HarnessStatus::PrerequisiteMissing);
        assert_eq!(record.responses_observed, 0);
        assert_eq!(record.degraded_reason.as_deref(), Some("socket missing"));
    }

    #[tokio::test]
    async fn insufficient_responses_fail_with_silence_reason() {
        let harness = ChatRoundtripHarness::new(
            StaticProbe(Ok(ChatRoundtripObservation {
                first_response_ms: 42,
                all_responses_ms: 77,
                responses_observed: 1,
                silence_reasons: vec!["helper-ai-only".to_string()],
                log_refs: vec!["airc://log/1".to_string()],
            })),
            ArtifactWriter::new(tempdir().unwrap().path()),
        );

        let record = harness.measure(config()).await;

        assert_eq!(record.status, HarnessStatus::Fail);
        assert_eq!(record.error_count, 1);
        assert_eq!(record.responses_observed, 1);
        assert_eq!(record.silence_reasons, ["helper-ai-only"]);
    }

    #[tokio::test]
    async fn successful_roundtrip_writes_jsonl_manifest_and_summary() {
        let dir = tempdir().unwrap();
        let harness = ChatRoundtripHarness::new(
            StaticProbe(Ok(ChatRoundtripObservation {
                first_response_ms: 40,
                all_responses_ms: 120,
                responses_observed: 2,
                silence_reasons: Vec::new(),
                log_refs: Vec::new(),
            })),
            ArtifactWriter::new(dir.path()),
        );

        let bundle = harness.run(config()).await.unwrap();

        let jsonl = std::fs::read_to_string(&bundle.record_jsonl).unwrap();
        let record: StandardVddRecord = serde_json::from_str(jsonl.trim()).unwrap();
        assert_eq!(record.status, HarnessStatus::Pass);
        assert_eq!(record.first_response_ms, Some(40));
        assert!(bundle.manifest_toml.exists());
        assert!(
            std::fs::read_to_string(&bundle.summary_md)
                .unwrap()
                .contains("chat-roundtrip-live-harness")
        );
    }
}
