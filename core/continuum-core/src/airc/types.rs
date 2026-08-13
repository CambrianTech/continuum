use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const DEFAULT_LIMIT: u16 = 20;
pub const MAX_LIMIT: u16 = 100;
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;
pub const MIN_TIMEOUT_MS: u64 = 100;
pub const MAX_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircQueueScanParams.ts"
)]
pub struct AircQueueScanParams {
    pub repo: String,
    #[ts(optional)]
    pub limit: Option<u16>,
    #[ts(optional)]
    pub owner: Option<String>,
    #[ts(optional)]
    pub status: Option<String>,
    #[ts(optional)]
    pub airc_bin: Option<String>,
    #[ts(optional)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircQueueCardEnvelope.ts"
)]
pub struct AircQueueCardEnvelope {
    pub kind: String,
    #[ts(optional)]
    pub id: Option<String>,
    #[ts(optional)]
    pub branch: Option<String>,
    #[ts(optional)]
    pub owner: Option<String>,
    pub status: String,
    #[ts(optional)]
    pub env: Option<String>,
    #[ts(optional)]
    pub evidence: Option<String>,
    #[ts(optional)]
    pub next_action: Option<String>,
    #[ts(optional)]
    pub last_heartbeat: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircQueueIssue.ts"
)]
pub struct AircQueueIssue {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
    pub card: AircQueueCardEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircQueueListEnvelope.ts"
)]
pub struct AircQueueListEnvelope {
    pub now_utc: String,
    pub repo: String,
    pub cards: Vec<AircQueueIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircQueueScanErrorKind.ts"
)]
pub enum AircQueueScanErrorKind {
    SpawnFailed,
    TimedOut,
    CommandFailed,
    InvalidJson,
    InvalidEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircQueueScanError.ts"
)]
pub struct AircQueueScanError {
    pub kind: AircQueueScanErrorKind,
    pub message: String,
    #[ts(optional)]
    pub exit_code: Option<i32>,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircQueueScanResult.ts"
)]
pub struct AircQueueScanResult {
    pub ok: bool,
    pub repo: String,
    pub card_count: usize,
    pub statuses: Vec<String>,
    pub owners: Vec<String>,
    pub command: Vec<String>,
    pub stdout_bytes: usize,
    pub stderr: String,
    #[ts(optional)]
    pub queue: Option<AircQueueListEnvelope>,
    #[ts(optional)]
    pub error: Option<AircQueueScanError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AircQueueListRequest {
    pub repo: String,
    pub limit: u16,
    pub owner: Option<String>,
    pub status: Option<String>,
    pub airc_bin: String,
    pub timeout_ms: u64,
}

impl TryFrom<AircQueueScanParams> for AircQueueListRequest {
    type Error = String;

    fn try_from(params: AircQueueScanParams) -> Result<Self, Self::Error> {
        validate_repo(&params.repo)?;

        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(format!("limit must be between 1 and {MAX_LIMIT}"));
        }

        let timeout_ms = params.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
        if !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
            return Err(format!(
                "timeout_ms must be between {MIN_TIMEOUT_MS} and {MAX_TIMEOUT_MS}"
            ));
        }

        let airc_bin = params.airc_bin.unwrap_or_else(|| "airc".to_string());
        if airc_bin.trim().is_empty() {
            return Err("airc_bin must not be empty".to_string());
        }

        Ok(Self {
            repo: params.repo,
            limit,
            owner: non_empty(params.owner),
            status: non_empty(params.status),
            airc_bin,
            timeout_ms,
        })
    }
}

impl AircQueueListRequest {
    pub fn args(&self) -> Vec<String> {
        let mut args = vec![
            "queue".to_string(),
            "list".to_string(),
            self.repo.clone(),
            "--limit".to_string(),
            self.limit.to_string(),
            "--json".to_string(),
        ];
        if let Some(owner) = &self.owner {
            args.push("--owner".to_string());
            args.push(owner.clone());
        }
        if let Some(status) = &self.status {
            args.push("--status".to_string());
            args.push(status.clone());
        }
        args
    }
}

pub fn command_vector(airc_bin: &str, args: &[String]) -> Vec<String> {
    let mut command = Vec::with_capacity(args.len() + 1);
    command.push(airc_bin.to_string());
    command.extend(args.iter().cloned());
    command
}

pub fn queue_failure_result(
    request: &AircQueueListRequest,
    args: &[String],
    kind: AircQueueScanErrorKind,
    message: String,
    exit_code: Option<i32>,
    stderr: String,
    stdout_bytes: usize,
) -> AircQueueScanResult {
    AircQueueScanResult {
        ok: false,
        repo: request.repo.clone(),
        card_count: 0,
        statuses: Vec::new(),
        owners: Vec::new(),
        command: command_vector(&request.airc_bin, args),
        stdout_bytes,
        stderr: stderr.clone(),
        queue: None,
        error: Some(AircQueueScanError {
            kind,
            message,
            exit_code,
            stderr,
        }),
    }
}

pub fn unique_card_field(
    cards: &[AircQueueIssue],
    field: impl Fn(&AircQueueIssue) -> Option<&str>,
) -> Vec<String> {
    let mut values = Vec::new();
    for card in cards {
        if let Some(value) = field(card) {
            if !values.iter().any(|seen| seen == value) {
                values.push(value.to_string());
            }
        }
    }
    values
}

fn validate_repo(repo: &str) -> Result<(), String> {
    let (owner, name) = repo
        .split_once('/')
        .ok_or_else(|| "repo must use owner/name form".to_string())?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return Err("repo must use owner/name form".to_string());
    }
    if !owner.chars().all(is_github_repo_char) || !name.chars().all(is_github_repo_char) {
        return Err("repo contains unsupported characters".to_string());
    }
    Ok(())
}

fn is_github_repo_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|inner| {
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_validation_rejects_stringly_bad_inputs() {
        assert!(AircQueueListRequest::try_from(AircQueueScanParams {
            repo: "not/a/repo".to_string(),
            limit: Some(20),
            owner: None,
            status: None,
            airc_bin: None,
            timeout_ms: None,
        })
        .is_err());

        assert!(AircQueueListRequest::try_from(AircQueueScanParams {
            repo: "CambrianTech/continuum".to_string(),
            limit: Some(0),
            owner: None,
            status: None,
            airc_bin: None,
            timeout_ms: None,
        })
        .is_err());
    }

    #[test]
    fn request_validation_trims_optional_filters() {
        let request = AircQueueListRequest::try_from(AircQueueScanParams {
            repo: "CambrianTech/continuum".to_string(),
            limit: None,
            owner: Some(" codex-main ".to_string()),
            status: Some(" ".to_string()),
            airc_bin: None,
            timeout_ms: None,
        })
        .unwrap();

        assert_eq!(request.limit, DEFAULT_LIMIT);
        assert_eq!(request.owner.as_deref(), Some("codex-main"));
        assert_eq!(request.status, None);
        assert_eq!(request.airc_bin, "airc");
    }
}
