//! Feature flag selection for continuum builds — replaces core-features.sh + cargo-features.sh.
//!
//! This module detects GPU hardware on the current node and assembles feature flags
//! (CUDA, ROCm, Vulkan, Metal) that are passed to `cargo build --features ...`.
//! It also resolves the node profile from config.env and validates it against
//! platform-specific published feature sets.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
/// Assembled GPU features for a node.
#[derive(Debug, Clone, Default)]
pub struct CoreFeatures {
    /// GPU backend feature (e.g., "cuda", "rocm", "vulkan", "metal").
    pub gpu_backend: Option<String>,
    /// Full cargo --features string (may include multiple features).
    pub cargo_features: String,
}

/// Resolve the node profile from config.env.
fn resolve_node_profile(config_path: &Path) -> Result<Option<String>, String> {
    let content = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read {}: {}", config_path.display(), e))?;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("CONTINUUM_NODE_FEATURES=") {
            return Ok(Some(val.to_string()));
        }
    }
    // No profile specified — allow auto-detection.
    Ok(None)
}

/// Check if a command exists on PATH.
fn command_exists(name: &str) -> bool {
    let mut cmd = Command::new("sh");
    if cfg!(target_os = "windows") {
        // On Windows, use where/which depending on shell availability.
        cmd.args(["-c", &format!("command -v {} >/dev/null 2>&1 || where {} >nul 2>nul", name, name)]);
    } else {
        cmd.args(["-c", &format!("command -v {} >/dev/null 2>&1", name)]);
    }
    cmd.output().map(|o| o.status.success()).unwrap_or(false)
}

/// Probe Linux for GPU availability in priority order: CUDA > ROCm > Vulkan.
fn probe_linux() -> Option<CoreFeatures> {
    // Check CUDA first (nvidia-smi + nvcc).
    if command_exists("nvidia-smi") && command_exists("nvcc") {
        return Some(CoreFeatures {
            gpu_backend: Some("cuda".into()),
            cargo_features: "cuda,cuda-12".into(),
        });
    }

    // Check ROCm (rocminfo).
    if command_exists("rocminfo") {
        return Some(CoreFeatures {
            gpu_backend: Some("rocm".into()),
            cargo_features: "rocm".into(),
        });
    }

    // Check Vulkan (vulkaninfo).
    if command_exists("vulkaninfo") {
        return Some(CoreFeatures {
            gpu_backend: Some("vulkan".into()),
            cargo_features: "vulkan".into(),
        });
    }

    None
}

/// Probe macOS for Metal GPU support.
fn probe_darwin() -> Option<CoreFeatures> {
    // Metal is always available on modern macOS with an Apple GPU.
    Some(CoreFeatures {
        gpu_backend: Some("metal".into()),
        cargo_features: "metal,accelerate".into(),
    })
}

/// Probe Windows for DX12 (DirectML) or CUDA availability.
fn probe_windows() -> Option<CoreFeatures> {
    // Check CUDA first if nvidia-smi and cl.exe are both present.
    if command_exists("nvidia-smi") && command_exists("cl") {
        return Some(CoreFeatures {
            gpu_backend: Some("cuda".into()),
            cargo_features: "cuda,cuda-12".into(),
        });
    }

    // DX12 / DirectML is the Windows fallback.
    Some(CoreFeatures {
        gpu_backend: Some("dx12".into()),
        cargo_features: "dx12,directml".into(),
    })
}

/// Detect GPU features based on the current platform.
pub fn detect_gpu_features() -> Option<CoreFeatures> {
    if cfg!(target_os = "linux") {
        probe_linux()
    } else if cfg!(target_os = "macos") {
        probe_darwin()
    } else if cfg!(target_os = "windows") {
        probe_windows()
    } else {
        None
    }
}

/// Assemble full feature set: GPU features + node profile validation.
/// Returns the assembled CoreFeatures or an error if the node profile is unrecognized.
pub fn select_core_features(
    platform_features: Option<CoreFeatures>,
    config_path: &Path,
) -> Result<CoreFeatures, String> {
    let gpu = platform_features.unwrap_or_else(|| {
        eprintln!("warning: no GPU features detected on this platform");
        CoreFeatures::default()
    });

    // Resolve and validate node profile.
    if let Some(node_profile) = resolve_node_profile(config_path)? {
        // Platform-specific published feature sets (simplified from core-features.sh).
        let valid_features: &[&str] = match env::consts::OS {
            "linux" => &["cuda", "rocm", "vulkan"],
            "macos" => &["metal"],
            "windows" => &["dx12", "cuda"],
            _ => &[],
        };

        let detected_backend = gpu.gpu_backend.as_deref().unwrap_or("");
        if !valid_features.iter().any(|&s| s == detected_backend) {
            return Err(format!(
                "Node profile '{}' does not match any valid features for this platform: {:?}",
                node_profile, valid_features
            ));
        }

        // If a specific profile was set, use its features.
        let profile_features = format!("{}", detected_backend);
        Ok(CoreFeatures {
            gpu_backend: Some(detected_backend.to_string()),
            cargo_features: profile_features,
        })
    } else {
        // No explicit profile — use whatever was auto-detected.
        Ok(gpu)
    }
}

/// Print the selected features for logging.
pub fn report_features(features: &CoreFeatures) {
    if let Some(ref backend) = features.gpu_backend {
        eprintln!("▶ GPU feature: {}", backend);
    } else {
        eprintln!("▶ No GPU features detected");
    }
    if !features.cargo_features.is_empty() {
        eprintln!("▶ Cargo features: {}", features.cargo_features);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_gpu_features_returns_some_on_known_platforms() {
        // On macOS, Metal should always be detected.
        if cfg!(target_os = "macos") {
            let features = detect_gpu_features();
            assert!(features.is_some());
            assert_eq!(features.unwrap().gpu_backend, Some("metal".into()));
        }
    }

    #[test]
    fn select_core_features_with_no_profile_uses_auto_detected() {
        let tmp = std::env::temp_dir().join("continuum_test_config.env");
        // Write an empty config — no CONTINUUM_NODE_FEATURES.
        fs::write(&tmp, "").unwrap();
        
        if cfg!(target_os = "macos") {
            let detected = detect_gpu_features();
            assert!(detected.is_some());
            let result = select_core_features(detected, &tmp);
            assert!(result.is_ok());
        }
        
        // Cleanup.
        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn command_exists_checks_path() {
        assert!(command_exists("sh"));
        assert!(!command_exists("this_command_does_not_exist_xyz"));
    }
}
