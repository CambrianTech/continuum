//! HeadTopology I/O, validation, and compatibility checks.
//!
//! The topology file (`head_topology.json`) is the manifest that tells
//! the inference engine how many heads per layer, which precision tier,
//! and what the original model dimensions were.

use super::types::*;
use std::path::Path;

/// Load a HeadTopology from a JSON file.
pub fn load_topology(path: &Path) -> Result<HeadTopology, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read topology file {}: {}", path.display(), e))?;
    let topology: HeadTopology = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse topology JSON: {e}"))?;
    validate_topology(&topology)?;
    Ok(topology)
}

/// Save a HeadTopology to a JSON file.
pub fn save_topology(topology: &HeadTopology, path: &Path) -> Result<(), String> {
    validate_topology(topology)?;
    let json = serde_json::to_string_pretty(topology)
        .map_err(|e| format!("Failed to serialize topology: {e}"))?;
    std::fs::write(path, json)
        .map_err(|e| format!("Failed to write topology to {}: {}", path.display(), e))?;
    Ok(())
}

/// Load utilization data (gate_gradients.json) from a file.
pub fn load_utilization_data(path: &Path) -> Result<UtilizationData, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read utilization data {}: {}", path.display(), e))?;
    let data: UtilizationData = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse utilization data JSON: {e}"))?;
    validate_utilization_data(&data)?;
    Ok(data)
}

/// Validate a HeadTopology for internal consistency.
pub fn validate_topology(topology: &HeadTopology) -> Result<(), String> {
    if topology.layers.is_empty() {
        return Err("Topology has no layers".to_string());
    }

    if topology.head_dim == 0 {
        return Err("head_dim must be > 0".to_string());
    }

    if topology.original_num_heads == 0 {
        return Err("original_num_heads must be > 0".to_string());
    }

    for (i, layer) in topology.layers.iter().enumerate() {
        if layer.layer_index != i {
            return Err(format!(
                "Layer index mismatch: expected {i}, got {}",
                layer.layer_index
            ));
        }

        if layer.num_heads != layer.retained_head_indices.len() {
            return Err(format!(
                "Layer {i}: num_heads ({}) != retained_head_indices.len() ({})",
                layer.num_heads,
                layer.retained_head_indices.len()
            ));
        }

        if layer.num_kv_heads != layer.retained_kv_head_indices.len() {
            return Err(format!(
                "Layer {i}: num_kv_heads ({}) != retained_kv_head_indices.len() ({})",
                layer.num_kv_heads,
                layer.retained_kv_head_indices.len()
            ));
        }

        if layer.head_precisions.len() != layer.num_heads {
            return Err(format!(
                "Layer {i}: head_precisions.len() ({}) != num_heads ({})",
                layer.head_precisions.len(),
                layer.num_heads
            ));
        }

        if layer.head_scores.len() != layer.num_heads {
            return Err(format!(
                "Layer {i}: head_scores.len() ({}) != num_heads ({})",
                layer.head_scores.len(),
                layer.num_heads
            ));
        }

        // Check retained indices are sorted and within bounds
        for (j, &idx) in layer.retained_head_indices.iter().enumerate() {
            if idx >= topology.original_num_heads {
                return Err(format!(
                    "Layer {i}: retained_head_indices[{j}] = {idx} >= original_num_heads ({})",
                    topology.original_num_heads
                ));
            }
            if j > 0 && idx <= layer.retained_head_indices[j - 1] {
                return Err(format!(
                    "Layer {i}: retained_head_indices not strictly ascending at position {j}"
                ));
            }
        }

        for (j, &idx) in layer.retained_kv_head_indices.iter().enumerate() {
            if idx >= topology.original_num_kv_heads {
                return Err(format!(
                    "Layer {i}: retained_kv_head_indices[{j}] = {idx} >= original_num_kv_heads ({})",
                    topology.original_num_kv_heads
                ));
            }
            if j > 0 && idx <= layer.retained_kv_head_indices[j - 1] {
                return Err(format!(
                    "Layer {i}: retained_kv_head_indices not strictly ascending at position {j}"
                ));
            }
        }

        // No Removed precision should appear in retained heads
        for (j, precision) in layer.head_precisions.iter().enumerate() {
            if *precision == HeadPrecision::Removed {
                return Err(format!(
                    "Layer {i}: head_precisions[{j}] is Removed but head is in retained list"
                ));
            }
        }
    }

    // Validate parameter reduction is in [0, 1)
    if topology.parameter_reduction < 0.0 || topology.parameter_reduction >= 1.0 {
        return Err(format!(
            "parameter_reduction must be in [0, 1), got {}",
            topology.parameter_reduction
        ));
    }

    Ok(())
}

/// Validate raw utilization data for consistency.
pub fn validate_utilization_data(data: &UtilizationData) -> Result<(), String> {
    if data.layer_scores.is_empty() {
        return Err("Utilization data has no layers".to_string());
    }

    if data.num_heads == 0 {
        return Err("num_heads must be > 0".to_string());
    }

    for (i, layer_scores) in data.layer_scores.iter().enumerate() {
        if layer_scores.len() != data.num_heads {
            return Err(format!(
                "Layer {i}: expected {} head scores, got {}",
                data.num_heads,
                layer_scores.len()
            ));
        }

        for (j, &score) in layer_scores.iter().enumerate() {
            if score < 0.0 {
                return Err(format!(
                    "Layer {i}, head {j}: utilization score is negative ({score})"
                ));
            }
            // Scores > 1.0 are valid (saturated heads)
        }
    }

    // GQA ratio must be an integer
    if data.num_kv_heads > 0 && data.num_heads % data.num_kv_heads != 0 {
        return Err(format!(
            "num_heads ({}) must be divisible by num_kv_heads ({})",
            data.num_heads, data.num_kv_heads
        ));
    }

    Ok(())
}

/// Check if a topology is compatible with a given base model config.
pub fn check_compatibility(
    topology: &HeadTopology,
    model_name: &str,
    expected_num_heads: usize,
    expected_num_kv_heads: usize,
    expected_head_dim: usize,
) -> Result<(), String> {
    if topology.base_model != model_name {
        return Err(format!(
            "Topology was created for '{}', but loading '{}'",
            topology.base_model, model_name
        ));
    }

    if topology.original_num_heads != expected_num_heads {
        return Err(format!(
            "Topology original_num_heads ({}) != model num_heads ({})",
            topology.original_num_heads, expected_num_heads
        ));
    }

    if topology.original_num_kv_heads != expected_num_kv_heads {
        return Err(format!(
            "Topology original_num_kv_heads ({}) != model num_kv_heads ({})",
            topology.original_num_kv_heads, expected_num_kv_heads
        ));
    }

    if topology.head_dim != expected_head_dim {
        return Err(format!(
            "Topology head_dim ({}) != model head_dim ({})",
            topology.head_dim, expected_head_dim
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_valid_topology() -> HeadTopology {
        HeadTopology {
            base_model: "test-model".to_string(),
            layers: vec![
                LayerTopology {
                    layer_index: 0,
                    num_heads: 3,
                    num_kv_heads: 3,
                    retained_head_indices: vec![0, 2, 3],
                    retained_kv_head_indices: vec![0, 2, 3],
                    head_precisions: vec![HeadPrecision::Q4, HeadPrecision::Q8, HeadPrecision::BF16],
                    head_scores: vec![0.15, 0.5, 0.85],
                },
            ],
            original_num_heads: 4,
            original_num_kv_heads: 4,
            head_dim: 64,
            parameter_reduction: 0.25,
            precision_profile: PrecisionProfile {
                removed: 1,
                q4: 1,
                q8: 1,
                bf16: 1,
            },
            created_at: "2026-03-16T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_validate_valid_topology() {
        let topology = make_valid_topology();
        assert!(validate_topology(&topology).is_ok());
    }

    #[test]
    fn test_validate_empty_layers() {
        let mut topology = make_valid_topology();
        topology.layers.clear();
        assert!(validate_topology(&topology).is_err());
    }

    #[test]
    fn test_validate_mismatched_head_count() {
        let mut topology = make_valid_topology();
        topology.layers[0].num_heads = 5; // Doesn't match retained_head_indices.len()
        assert!(validate_topology(&topology).is_err());
    }

    #[test]
    fn test_validate_out_of_bounds_index() {
        let mut topology = make_valid_topology();
        topology.layers[0].retained_head_indices[2] = 99; // Way out of bounds
        assert!(validate_topology(&topology).is_err());
    }

    #[test]
    fn test_validate_unsorted_indices() {
        let mut topology = make_valid_topology();
        topology.layers[0].retained_head_indices = vec![3, 2, 0]; // Not ascending
        assert!(validate_topology(&topology).is_err());
    }

    #[test]
    fn test_validate_removed_in_retained() {
        let mut topology = make_valid_topology();
        topology.layers[0].head_precisions[0] = HeadPrecision::Removed;
        assert!(validate_topology(&topology).is_err());
    }

    #[test]
    fn test_validate_bad_parameter_reduction() {
        let mut topology = make_valid_topology();
        topology.parameter_reduction = 1.0; // Must be < 1.0
        assert!(validate_topology(&topology).is_err());
    }

    #[test]
    fn test_validate_utilization_data_ok() {
        let data = UtilizationData {
            layer_scores: vec![vec![0.5, 0.3, 0.8, 0.1]],
            num_steps: 100,
            model_name: "test".to_string(),
            num_heads: 4,
            num_kv_heads: 4,
        };
        assert!(validate_utilization_data(&data).is_ok());
    }

    #[test]
    fn test_validate_utilization_wrong_head_count() {
        let data = UtilizationData {
            layer_scores: vec![vec![0.5, 0.3]], // Only 2 scores but num_heads = 4
            num_steps: 100,
            model_name: "test".to_string(),
            num_heads: 4,
            num_kv_heads: 4,
        };
        assert!(validate_utilization_data(&data).is_err());
    }

    #[test]
    fn test_validate_utilization_negative_score() {
        let data = UtilizationData {
            layer_scores: vec![vec![0.5, -0.1, 0.8, 0.1]],
            num_steps: 100,
            model_name: "test".to_string(),
            num_heads: 4,
            num_kv_heads: 4,
        };
        assert!(validate_utilization_data(&data).is_err());
    }

    #[test]
    fn test_validate_utilization_gqa_mismatch() {
        let data = UtilizationData {
            layer_scores: vec![vec![0.5; 6]],
            num_steps: 100,
            model_name: "test".to_string(),
            num_heads: 6,
            num_kv_heads: 4, // 6 is not divisible by 4
        };
        assert!(validate_utilization_data(&data).is_err());
    }

    #[test]
    fn test_roundtrip_topology_file() {
        let topology = make_valid_topology();
        let tmpfile = NamedTempFile::new().unwrap();
        let path = tmpfile.path().to_path_buf();

        save_topology(&topology, &path).unwrap();
        let loaded = load_topology(&path).unwrap();

        assert_eq!(loaded.base_model, topology.base_model);
        assert_eq!(loaded.layers.len(), topology.layers.len());
        assert_eq!(loaded.original_num_heads, topology.original_num_heads);
        assert_eq!(loaded.head_dim, topology.head_dim);
        assert_eq!(loaded.layers[0].num_heads, topology.layers[0].num_heads);
    }

    #[test]
    fn test_load_topology_bad_json() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        write!(tmpfile, "not json").unwrap();
        let result = load_topology(tmpfile.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_topology_missing_file() {
        let result = load_topology(Path::new("/nonexistent/path/topology.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_check_compatibility_ok() {
        let topology = make_valid_topology();
        assert!(check_compatibility(&topology, "test-model", 4, 4, 64).is_ok());
    }

    #[test]
    fn test_check_compatibility_wrong_model() {
        let topology = make_valid_topology();
        assert!(check_compatibility(&topology, "other-model", 4, 4, 64).is_err());
    }

    #[test]
    fn test_check_compatibility_wrong_heads() {
        let topology = make_valid_topology();
        assert!(check_compatibility(&topology, "test-model", 8, 4, 64).is_err());
    }
}
