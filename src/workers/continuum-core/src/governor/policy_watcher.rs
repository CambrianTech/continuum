//! Policy directory discovery and hot reload for `LocalSubstrateGovernor`.
//!
//! This module is deliberately small: it loads TOML policy files through
//! `policy_file`, swaps the fully parsed candidate set into the governor,
//! and keeps a `notify` watcher alive so operator edits can trigger the
//! same reload path. Broken directories or malformed files return typed
//! errors. The watcher callback records and logs failures instead of
//! replacing a good candidate set with junk.

use crate::governor::{LocalSubstrateGovernor, PolicyFile, PolicyFileError, load_policy_file};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum PolicyDirectoryError {
    #[error("policy directory I/O failed at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("policy file failed to load at {path}: {source}")]
    Policy {
        path: PathBuf,
        #[source]
        source: PolicyFileError,
    },
    #[error("policy directory {path} has no .toml policy files")]
    Empty { path: PathBuf },
    #[error("policy watcher failed for {path}: {source}")]
    Watch {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },
}

pub struct PolicyDirectoryWatcher {
    _watcher: RecommendedWatcher,
    policy_dir: PathBuf,
    governor: Arc<LocalSubstrateGovernor>,
    last_error: Arc<Mutex<Option<String>>>,
}

impl PolicyDirectoryWatcher {
    pub fn policy_dir(&self) -> &Path {
        &self.policy_dir
    }

    pub fn candidate_count(&self) -> usize {
        self.governor.candidate_count()
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .expect("PolicyDirectoryWatcher last_error mutex poisoned")
            .clone()
    }

    pub fn reload_now(&self) -> Result<usize, PolicyDirectoryError> {
        reload_policy_candidates(&self.governor, &self.policy_dir)
    }

    pub fn clear_last_error(&self) {
        let mut guard = self
            .last_error
            .lock()
            .expect("PolicyDirectoryWatcher last_error mutex poisoned");
        *guard = None;
    }
}

pub fn watch_policy_directory(
    policy_dir: impl AsRef<Path>,
    governor: Arc<LocalSubstrateGovernor>,
) -> Result<PolicyDirectoryWatcher, PolicyDirectoryError> {
    let policy_dir = policy_dir.as_ref().to_path_buf();
    reload_policy_candidates(&governor, &policy_dir)?;

    let last_error = Arc::new(Mutex::new(None));
    let callback_dir = policy_dir.clone();
    let callback_governor = Arc::clone(&governor);
    let callback_last_error = Arc::clone(&last_error);

    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let result = match event {
            Ok(event) if is_reload_event(&event) => {
                reload_policy_candidates(&callback_governor, &callback_dir).map(|_| ())
            }
            Ok(_) => Ok(()),
            Err(source) => Err(PolicyDirectoryError::Watch {
                path: callback_dir.clone(),
                source,
            }),
        };

        if let Err(error) = result {
            let message = error.to_string();
            tracing::error!(target: "continuum_core::governor::policy_watcher", %message);
            let mut guard = callback_last_error
                .lock()
                .expect("PolicyDirectoryWatcher last_error mutex poisoned");
            *guard = Some(message);
        }
    })
    .map_err(|source| PolicyDirectoryError::Watch {
        path: policy_dir.clone(),
        source,
    })?;

    watcher
        .watch(&policy_dir, RecursiveMode::NonRecursive)
        .map_err(|source| PolicyDirectoryError::Watch {
            path: policy_dir.clone(),
            source,
        })?;

    Ok(PolicyDirectoryWatcher {
        _watcher: watcher,
        policy_dir,
        governor,
        last_error,
    })
}

pub fn reload_policy_candidates(
    governor: &LocalSubstrateGovernor,
    policy_dir: &Path,
) -> Result<usize, PolicyDirectoryError> {
    let policies = load_policy_directory(policy_dir)?;
    let count = policies.len();
    governor.set_candidates(policies);
    Ok(count)
}

pub fn load_policy_directory(policy_dir: &Path) -> Result<Vec<PolicyFile>, PolicyDirectoryError> {
    let mut paths = Vec::new();
    let entries = std::fs::read_dir(policy_dir).map_err(|source| PolicyDirectoryError::Io {
        path: policy_dir.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| PolicyDirectoryError::Io {
            path: policy_dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("toml") {
            paths.push(path);
        }
    }

    paths.sort();
    if paths.is_empty() {
        return Err(PolicyDirectoryError::Empty {
            path: policy_dir.to_path_buf(),
        });
    }

    paths
        .into_iter()
        .map(|path| {
            load_policy_file(&path).map_err(|source| PolicyDirectoryError::Policy { path, source })
        })
        .collect()
}

fn is_reload_event(event: &Event) -> bool {
    let touches_policy = event.paths.is_empty()
        || event
            .paths
            .iter()
            .any(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"));

    touches_policy
        && matches!(
            event.kind,
            EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governor::types::{
        CadenceMultipliers, ConcurrencyCaps, ConsolidationSchedule, FederationCadence,
        GovernorPolicy, HardwareClass, PowerSource, RecallScoreWeights, SpeculationLevel,
        TargetSilicon, ThermalClass, TierSizes,
    };
    use notify::event::{AccessKind, CreateKind};

    const AIR_POLICY: &str = r#"
policy_version = 3
applies_to    = "apple-m,thinandlight,uma,vram_mb=0..0,ram_mb=14000..18000"

[tier_sizes]
l1_lora_layers       = 2
l1_kv_tokens         = 2048
l2_lora_layers       = 4
l3_lora_layers       = 12
l3_engrams           = 1024

[cadence_multipliers]
realtime             = 1.0
delayed              = 1.5
background           = 2.0

[concurrency_caps]
personas_concurrent  = 2
inference_lanes      = 1
foundry_lanes        = 0
sentinel_lanes       = 1

[speculation]
level                = "conservative"

[consolidation]
schedule             = "idle-plugged-in"

[federation]
pull_cadence_seconds = 600

[recall_weights]
semantic             = 0.4
outcome_history      = 0.3
recency              = 0.1
tier_proximity       = 0.1
provenance_trust     = 0.1
"#;

    const NVIDIA_POLICY: &str = r#"
policy_version = 1
applies_to     = "nvidia,workstation,vram_mb=30000..36000,ram_mb=60000..80000"

[tier_sizes]
l1_lora_layers        = 8
l1_kv_tokens          = 16384
l2_lora_layers        = 16
l3_lora_layers        = 40
l3_engrams            = 10240

[cadence_multipliers]
realtime              = 1.0
delayed               = 1.0
background            = 1.5

[concurrency_caps]
personas_concurrent   = 8
inference_lanes       = 4
foundry_lanes         = 1
sentinel_lanes        = 2

[speculation]
level                 = "aggressive"

[consolidation]
schedule              = "idle"

[federation]
pull_cadence_seconds  = 60

[recall_weights]
semantic              = 0.4
outcome_history       = 0.3
recency               = 0.1
tier_proximity        = 0.1
provenance_trust      = 0.1
"#;

    #[test]
    fn load_policy_directory_loads_sorted_toml_only() {
        let dir = tempfile::tempdir().expect("tempdir should be creatable");
        write(dir.path().join("b-nvidia.toml"), NVIDIA_POLICY);
        write(dir.path().join("a-air.toml"), AIR_POLICY);
        write(dir.path().join("notes.txt"), "ignored");

        let policies = load_policy_directory(dir.path()).expect("policies should load");

        assert_eq!(policies.len(), 2);
        assert_eq!(policies[0].policy_version, 3);
        assert_eq!(policies[1].policy_version, 1);
    }

    #[test]
    fn load_policy_directory_empty_dir_fails_loud() {
        let dir = tempfile::tempdir().expect("tempdir should be creatable");

        let result = load_policy_directory(dir.path());

        assert!(matches!(result, Err(PolicyDirectoryError::Empty { .. })));
    }

    #[test]
    fn load_policy_directory_invalid_policy_identifies_path() {
        let dir = tempfile::tempdir().expect("tempdir should be creatable");
        let bad_path = dir.path().join("bad.toml");
        write(&bad_path, "not valid [[[");

        let result = load_policy_directory(dir.path());

        match result {
            Err(PolicyDirectoryError::Policy { path, source }) => {
                assert_eq!(path, bad_path);
                assert!(matches!(source, PolicyFileError::Toml(_)));
            }
            other => panic!("expected policy parse error, got {other:?}"),
        }
    }

    #[test]
    fn reload_policy_candidates_replaces_candidate_pool_atomically() {
        let dir = tempfile::tempdir().expect("tempdir should be creatable");
        write(dir.path().join("air.toml"), AIR_POLICY);
        write(dir.path().join("nvidia.toml"), NVIDIA_POLICY);
        let governor = LocalSubstrateGovernor::new(initial_policy());

        let count =
            reload_policy_candidates(&governor, dir.path()).expect("valid policies should reload");

        assert_eq!(count, 2);
        assert_eq!(governor.candidate_count(), 2);
    }

    #[test]
    fn reload_policy_candidates_keeps_existing_pool_on_error() {
        let valid_dir = tempfile::tempdir().expect("tempdir should be creatable");
        write(valid_dir.path().join("air.toml"), AIR_POLICY);
        let bad_dir = tempfile::tempdir().expect("tempdir should be creatable");
        write(bad_dir.path().join("bad.toml"), "not valid [[[");
        let governor = LocalSubstrateGovernor::new(initial_policy());
        reload_policy_candidates(&governor, valid_dir.path())
            .expect("valid policies should reload first");

        let result = reload_policy_candidates(&governor, bad_dir.path());

        assert!(matches!(result, Err(PolicyDirectoryError::Policy { .. })));
        assert_eq!(governor.candidate_count(), 1);
    }

    #[test]
    fn watch_policy_directory_initial_loads_candidates() {
        let dir = tempfile::tempdir().expect("tempdir should be creatable");
        write(dir.path().join("air.toml"), AIR_POLICY);
        let governor = Arc::new(LocalSubstrateGovernor::new(initial_policy()));

        let watcher = watch_policy_directory(dir.path(), Arc::clone(&governor))
            .expect("valid directory should start watcher");

        assert_eq!(watcher.policy_dir(), dir.path());
        assert_eq!(watcher.candidate_count(), 1);
        assert_eq!(watcher.last_error(), None);
    }

    #[test]
    fn watcher_reload_now_uses_same_strict_loader() {
        let dir = tempfile::tempdir().expect("tempdir should be creatable");
        write(dir.path().join("air.toml"), AIR_POLICY);
        let governor = Arc::new(LocalSubstrateGovernor::new(initial_policy()));
        let watcher = watch_policy_directory(dir.path(), Arc::clone(&governor))
            .expect("valid directory should start watcher");
        write(dir.path().join("nvidia.toml"), NVIDIA_POLICY);

        let count = watcher
            .reload_now()
            .expect("manual reload should load both");

        assert_eq!(count, 2);
        assert_eq!(governor.candidate_count(), 2);
    }

    #[test]
    fn is_reload_event_requires_policy_file_and_write_kind() {
        let toml_create = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("policy.toml")],
            attrs: Default::default(),
        };
        let txt_create = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("notes.txt")],
            attrs: Default::default(),
        };
        let toml_access = Event {
            kind: EventKind::Access(AccessKind::Any),
            paths: vec![PathBuf::from("policy.toml")],
            attrs: Default::default(),
        };

        assert!(is_reload_event(&toml_create));
        assert!(!is_reload_event(&txt_create));
        assert!(!is_reload_event(&toml_access));
    }

    fn write(path: impl AsRef<Path>, text: &str) {
        std::fs::write(path, text).expect("test file should be writable");
    }

    fn initial_policy() -> GovernorPolicy {
        GovernorPolicy {
            policy_version: 1,
            hardware_class: HardwareClass {
                silicon: TargetSilicon::AppleM,
                silicon_model: "M2".to_string(),
                vram_mb: 0,
                system_ram_mb: 16_384,
                thermal_class: ThermalClass::ThinAndLight,
                power_source: PowerSource::Battery,
                battery_pct: Some(80),
                thermal_headroom_pct: Some(60),
            },
            tier_sizes: TierSizes {
                l1_lora_layers: 2,
                l1_kv_tokens: 2048,
                l2_lora_layers: 4,
                l3_lora_layers: 12,
                l3_engrams: 1024,
            },
            cadence_multipliers: CadenceMultipliers {
                realtime: 1.0,
                delayed: 1.5,
                background: 2.0,
            },
            concurrency_caps: ConcurrencyCaps {
                personas_concurrent: 2,
                inference_lanes: 1,
                foundry_lanes: 0,
                sentinel_lanes: 1,
            },
            speculation_aggressiveness: SpeculationLevel::Conservative,
            consolidation_schedule: ConsolidationSchedule::IdlePluggedIn,
            federation_pull_cadence: FederationCadence {
                pull_cadence_seconds: 600,
            },
            recall_score_weights: RecallScoreWeights {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
            cascade_step: 0,
            committed_at_ms: 1,
        }
    }
}
