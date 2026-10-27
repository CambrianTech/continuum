//! Shared deployment discovery: bounded build history and platform publication probes.
//! Tracking selects an authorized candidate; only the consumer downloads and verifies it.

use continuum_cli_lifecycle::{
    deploy_tracker::Checks,
    prebuilt_artifact::{select_published_core, PublishedCoreError, PublishedCoreSource},
};
use std::{path::Path, time::Duration};

/// Both tracking and consumption walk the same bounded build-input history.
/// A history read failure retains the newest key alone, never an invented ancestor.
pub fn deploy_candidates(
    repo: &Path,
    running: Option<&str>,
    tip: &str,
    newest_key: &str,
) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(running) = running {
        let mut args = vec!["-C".to_string(), repo.to_string_lossy().into_owned()];
        args.extend(
            continuum_cli_lifecycle::prebuilt_artifact::candidate_keys_log_args(running, tip),
        );
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out =
            crate::system_resources::bounded_command::probe("git", &args, Duration::from_secs(30));
        if let Some(text) = out.stdout_if_ok() {
            keys = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect();
        } else {
            continuum_base::probe!(
                class = "deploy.candidates.unavailable",
                outcome = out.outcome(),
                "cannot walk build history; retaining only the newest build key"
            );
        }
    }
    keys.retain(|key| key != newest_key);
    keys.insert(0, newest_key.to_string());
    keys
}

/// Has CI published `key`'s core for `platform`? One small request for the manifest: the
/// archive is fetched only for the key chosen.
pub async fn ci_core_published(key: &str, platform: &str) -> Result<bool, String> {
    let url = continuum_cli_lifecycle::prebuilt_artifact::manifest_url(key, platform)
        .ok_or_else(|| format!("{key} is not a full commit sha"))?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    // GET, not HEAD: a release asset redirects to storage signed for GET, which can refuse
    // a HEAD; the manifest is a few hundred bytes.
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("asking {url}: {e}"))?;
    match resp.status() {
        reqwest::StatusCode::NOT_FOUND => Ok(false),
        status if status.is_success() => Ok(true),
        status => Err(format!("asking {url}: {status}")),
    }
}

/// A preflight adapter: the shared selector proves publication and exact-revision CI.
/// Its result authorizes a request, not installation. The consumer rechecks both and
/// validates the manifest, payload, platform and hash before any running binary swap.
struct CandidateProbe<'a> {
    repo: &'a Path,
    platform: &'a str,
}

#[async_trait::async_trait]
impl PublishedCoreSource for CandidateProbe<'_> {
    type Artifact = ();
    async fn published(&self, key: &str) -> Result<bool, String> {
        ci_core_published(key, self.platform).await
    }
    async fn checks(&self, key: &str) -> Result<Checks, String> {
        let repo = self.repo.to_path_buf();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || super::deploy_tracker::revision_checks(&repo, &key))
            .await
            .map_err(|e| format!("candidate checks task: {e}"))?
    }
    async fn fetch(&self, _key: &str) -> Result<Option<()>, PublishedCoreError> {
        Ok(Some(()))
    }
}

pub async fn deployable_candidate(
    repo: &Path,
    candidates: &[String],
) -> Result<Option<String>, String> {
    let platform = continuum_cli_lifecycle::prebuilt_artifact::platform_key(
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
    .ok_or_else(|| "no supported prebuilt platform for this node".to_string())?;
    selected_candidate(candidates, &CandidateProbe { repo, platform }).await
}

async fn selected_candidate<S: PublishedCoreSource>(
    candidates: &[String],
    source: &S,
) -> Result<Option<String>, String> {
    match select_published_core(candidates, source).await {
        Some((key, Ok(Some(_)))) => Ok(Some(key)),
        Some((_, Err(why))) => Err(why.to_string()),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_cli_lifecycle::deploy_tracker::{decide, DeployVerdict, TickInputs};

    struct Source {
        ancestor: Option<Checks>,
        fetched: std::sync::Mutex<Vec<String>>,
    }
    #[async_trait::async_trait]
    impl PublishedCoreSource for Source {
        type Artifact = ();
        async fn published(&self, key: &str) -> Result<bool, String> {
            Ok(key == "new-tip" || self.ancestor.is_some())
        }
        async fn checks(&self, key: &str) -> Result<Checks, String> {
            if key == "new-tip" {
                Ok(Checks::Pending)
            } else {
                self.ancestor
                    .ok_or_else(|| "unpublished ancestor".to_string())
            }
        }
        async fn fetch(&self, key: &str) -> Result<Option<()>, PublishedCoreError> {
            self.fetched.lock().unwrap().push(key.to_string());
            Ok(Some(()))
        }
    }

    async fn tick(source: &Source) -> DeployVerdict {
        let selected =
            selected_candidate(&["new-tip".into(), "published-ancestor".into()], source).await;
        let (tip_sha, source_error, checks) = match selected {
            Ok(Some(sha)) => (Some(sha), None, Checks::Green),
            Ok(None) => (Some("new-tip".into()), None, Checks::Unknown),
            Err(why) => (None, Some(why), Checks::Unknown),
        };
        decide(&TickInputs {
            running_sha: Some("old-running".into()),
            tip_sha,
            tip_build_key: None,
            source_error,
            checks,
            hold: None,
            build_in_flight: false,
            tree_dirty: false,
            now_ms: 10,
        })
    }

    #[tokio::test]
    async fn pending_tip_does_not_starve_published_green_ancestor_request() {
        let source = Source {
            ancestor: Some(Checks::Green),
            fetched: Default::default(),
        };
        assert_eq!(
            tick(&source).await,
            DeployVerdict::Deploy {
                tip_sha: "published-ancestor".into()
            }
        );
        assert_eq!(*source.fetched.lock().unwrap(), vec!["published-ancestor"]);
    }

    #[tokio::test]
    async fn no_eligible_candidate_never_requests_deploy() {
        for ancestor in [
            None,
            Some(Checks::Unknown),
            Some(Checks::Pending),
            Some(Checks::Red),
        ] {
            let source = Source {
                ancestor,
                fetched: Default::default(),
            };
            assert!(matches!(
                tick(&source).await,
                DeployVerdict::SourceUnavailable { .. } | DeployVerdict::ChecksUnknown
            ));
            assert!(source.fetched.lock().unwrap().is_empty());
        }
    }
}
