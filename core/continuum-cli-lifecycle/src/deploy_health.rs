//! Durable refusal evidence carried by the existing deploy consumer ledger.
//! The consumer owns writes; health readers must not clear evidence or retry a deploy.

use serde::{Deserialize, Serialize};

/// The consumer's resolved build key for one request, retained after adoption.
/// A docs-only tip may name an older binary; unrelated tips cannot reuse this proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploySelection {
    pub request: String,
    pub build: String,
}

impl DeploySelection {
    pub fn satisfies(&self, request: &str, running: &str) -> bool {
        self.request == request
            && crate::deploy_tracker::running_satisfies(running, request, Some(&self.build))
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DeployHealth {
    NoRequest,
    Running,
    Blocked {
        refusal: DeployRefusal,
    },
    Pending {
        request: crate::deploy_tracker::DeployRequest,
    },
    Unknown {
        reason: String,
    },
}

/// Read the existing request and consumer receipt. Missing evidence while a
/// request stands is unknown, including when a full disk prevented its write.
/// This projection never changes either file or takes ownership from the consumer.
pub fn read(root: &std::path::Path, running: &str) -> DeployHealth {
    let state = root.join("state");
    let request_text = match std::fs::read_to_string(state.join("deploy-request.json")) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return DeployHealth::NoRequest,
        Err(e) => {
            return DeployHealth::Unknown {
                reason: format!("cannot read deploy request: {e}"),
            }
        }
    };
    let request: crate::deploy_tracker::DeployRequest = match serde_json::from_str(&request_text) {
        Ok(value) => value,
        Err(e) => {
            return DeployHealth::Unknown {
                reason: format!("invalid deploy request: {e}"),
            }
        }
    };
    if crate::deploy_tracker::running_satisfies(running, &request.tip_sha, None) {
        return DeployHealth::Running;
    }
    let text = match std::fs::read_to_string(state.join("deploy-consume-attempts.json")) {
        Ok(text) => text,
        Err(e) => {
            return DeployHealth::Unknown {
                reason: format!(
                    "deploy {} pending without readable consumer evidence: {e}",
                    request.tip_sha
                ),
            }
        }
    };
    #[derive(Deserialize)]
    struct Receipt {
        // Required maps validate the current ledger shape; old receipts are not
        // silently interpreted as a healthy new consumer.
        failures_by_build: std::collections::BTreeMap<String, u32>,
        resource_deferrals:
            std::collections::BTreeMap<String, crate::prebuilt_artifact::ResourceDeferral>,
        refusal: Option<DeployRefusal>,
        selection: Option<DeploySelection>,
    }
    let receipt: Receipt = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            return DeployHealth::Unknown {
                reason: format!("invalid deploy consumer evidence: {e}"),
            }
        }
    };
    if receipt
        .selection
        .as_ref()
        .is_some_and(|s| s.satisfies(&request.tip_sha, running))
    {
        return DeployHealth::Running;
    }
    if let Some(refusal) = receipt.refusal {
        if refusal.request == request.tip_sha && !refusal.resolved_by(running) {
            return DeployHealth::Blocked { refusal };
        }
    }
    if !receipt.resource_deferrals.is_empty()
        || receipt.failures_by_build.values().any(|count| *count > 0)
    {
        return DeployHealth::Unknown {
            reason: "deploy failures exist without a matching current refusal receipt".into(),
        };
    }
    DeployHealth::Pending { request }
}

impl DeployHealth {
    pub fn line(&self, now_ms: u64) -> String {
        match self {
            Self::NoRequest | Self::Running => String::new(),
            Self::Blocked { refusal } => format!(
                " · deploy blocked: {:?} for {}s: {}",
                refusal.kind,
                now_ms.saturating_sub(refusal.since_ms) / 1_000,
                refusal.reason
            ),
            Self::Pending { request } => format!(
                " · deploy pending: {} for {}s",
                request.tip_sha,
                now_ms.saturating_sub(request.requested_ms) / 1_000
            ),
            Self::Unknown { reason } => format!(" · deploy state unknown: {reason}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalKind {
    /// A particular published candidate has not passed source authorization yet.
    SourceAuthorization,
    ResourceAdmission,
    CheckoutDirty,
    AttemptsExhausted,
    HandoffFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployRefusal {
    pub request: String,
    pub build: String,
    pub kind: RefusalKind,
    pub reason: String,
    pub since_ms: u64,
    pub observed_ms: u64,
}

impl DeployRefusal {
    /// A retry updates evidence without making a continuing refusal look new.
    /// A different requested or attempted build starts a distinct incident.
    pub fn observe(
        previous: Option<&Self>,
        request: &str,
        build: &str,
        kind: RefusalKind,
        reason: &str,
        now_ms: u64,
    ) -> Self {
        let since_ms = match previous {
            Some(old) if old.request == request && old.build == build && old.kind == kind => {
                old.since_ms
            }
            _ => now_ms,
        };
        Self {
            request: request.into(),
            build: build.into(),
            kind,
            reason: reason.into(),
            since_ms,
            observed_ms: now_ms,
        }
    }

    /// Installer handoff is not confirmation. Only an observed running revision
    /// satisfying this incident's requested build resolves the evidence.
    pub fn resolved_by(&self, running: &str) -> bool {
        crate::deploy_tracker::running_satisfies(running, &self.request, Some(&self.build))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: lost/full-disk or corrupt evidence being reported healthy,
    // and a health read mutating the consumer's durable incident.
    #[test]
    fn projection_requires_evidence_and_never_clears_it() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = tempfile::tempdir()?;
        let state = root.path().join("state");
        std::fs::create_dir(&state)?;
        let request_path = state.join("deploy-request.json");
        let ledger_path = state.join("deploy-consume-attempts.json");
        std::fs::write(&request_path, r#"{"tip_sha":"docs-tip","requested_ms":10}"#)?;
        assert!(matches!(
            read(root.path(), "old"),
            DeployHealth::Unknown { .. }
        ));
        std::fs::write(&ledger_path, "{")?;
        assert!(matches!(
            read(root.path(), "old"),
            DeployHealth::Unknown { .. }
        ));
        let refusal = DeployRefusal::observe(
            None,
            "docs-tip",
            "core-build",
            RefusalKind::ResourceAdmission,
            "disk full",
            10,
        );
        let evidence = serde_json::to_vec(&serde_json::json!({
            "failures_by_build": {}, "resource_deferrals": {}, "refusal": refusal,
            "selection": {"request": "docs-tip", "build": "core-build"}
        }))?;
        std::fs::write(&ledger_path, &evidence)?;
        assert!(matches!(
            read(root.path(), "old"),
            DeployHealth::Blocked { .. }
        ));
        assert!(matches!(
            read(root.path(), "core-build"),
            DeployHealth::Running
        ));
        assert_eq!(std::fs::read(&ledger_path)?, evidence);
        std::fs::write(&request_path, r#"{"tip_sha":"new-tip","requested_ms":20}"#)?;
        assert!(matches!(
            read(root.path(), "core-build"),
            DeployHealth::Pending { .. }
        ));
        Ok(())
    }

    // An older fallback can recover serving without satisfying the requested tip.
    #[test]
    fn fallback_adoption_does_not_report_the_newer_request_running(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let state = root.path().join("state");
        std::fs::create_dir(&state)?;
        std::fs::write(
            state.join("deploy-request.json"),
            r#"{"tip_sha":"newer-tip","requested_ms":10}"#,
        )?;
        let refusal = DeployRefusal::observe(
            None,
            "newer-tip",
            "old-build",
            RefusalKind::HandoffFailed,
            "fallback failed",
            10,
        );
        std::fs::write(
            state.join("deploy-consume-attempts.json"),
            serde_json::to_vec(&serde_json::json!({
                "failures_by_build": {}, "resource_deferrals": {}, "refusal": refusal,
                "selection": {"request": "newer-tip", "build": "new-build"}
            }))?,
        )?;
        assert!(matches!(
            read(root.path(), "old-build"),
            DeployHealth::Pending { .. }
        ));
        assert!(matches!(
            read(root.path(), "new-build"),
            DeployHealth::Running
        ));
        Ok(())
    }

    // what this catches: a docs-only request stays pending after its binary lands,
    // or an old request's build mapping incorrectly satisfies a superseding request.
    #[test]
    fn selected_build_proves_only_its_own_request() {
        let selection = DeploySelection {
            request: "docs-tip".into(),
            build: "core-build".into(),
        };
        assert!(selection.satisfies("docs-tip", "core-build"));
        assert!(!selection.satisfies("new-tip", "core-build"));
        assert!(!selection.satisfies("docs-tip", "old-build"));
    }

    // what this catches: retries hiding incident age or another build clearing a refusal.
    #[test]
    fn refusal_retains_age_until_the_matching_revision_runs() {
        let first = DeployRefusal::observe(
            None,
            "request-a",
            "build-a",
            RefusalKind::ResourceAdmission,
            "disk",
            10,
        );
        let retry = DeployRefusal::observe(
            Some(&first),
            "request-a",
            "build-a",
            RefusalKind::ResourceAdmission,
            "disk still full",
            40,
        );
        assert_eq!(retry.since_ms, 10);
        assert_eq!(retry.observed_ms, 40);
        assert!(!retry.resolved_by("build-b"));
        assert!(retry.resolved_by("build-a"));
        let next = DeployRefusal::observe(
            Some(&retry),
            "request-b",
            "build-b",
            RefusalKind::ResourceAdmission,
            "disk",
            50,
        );
        assert_eq!(next.since_ms, 50);
    }
}
