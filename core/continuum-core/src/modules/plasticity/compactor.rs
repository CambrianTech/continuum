//! Tensor slicing: physically remove pruned heads from safetensors.
//!
//! Supports both single-file and multi-shard safetensors (e.g., model-00001-of-00007.safetensors).
//!
//! For Llama-3.2-3B (24 Q heads, 8 KV heads, head_dim=128, hidden=3072):
//!
//! ```text
//! q_proj.weight: [3072, 3072] → remove rows for pruned heads → [retained*128, 3072]
//! k_proj.weight: [1024, 3072] → remove rows for pruned KV heads → [retained_kv*128, 3072]
//! v_proj.weight: [1024, 3072] → same as k_proj
//! o_proj.weight: [3072, 3072] → remove columns for pruned heads → [3072, retained*128]
//! ```
//!
//! Bias vectors sliced correspondingly. MLP weights, layer norms, embeddings copied verbatim.

use super::types::*;
use safetensors::tensor::TensorView;
use safetensors::{Dtype, SafeTensors};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Discover all safetensors shard files for a model directory.
///
/// Handles both:
/// - Single file: `model.safetensors`
/// - Multi-shard: `model-00001-of-00007.safetensors`, etc.
///
/// Returns shard paths sorted by shard number.
pub fn discover_shards(model_dir: &Path) -> Result<Vec<PathBuf>, String> {
    // Check for single file first
    let single = model_dir.join("model.safetensors");
    if single.exists() {
        return Ok(vec![single]);
    }

    // Check for multi-shard pattern
    let mut shards: Vec<PathBuf> = Vec::new();
    let entries = std::fs::read_dir(model_dir).map_err(|e| {
        format!(
            "Failed to read model directory {}: {}",
            model_dir.display(),
            e
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read dir entry: {e}"))?;
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Match pattern: model-NNNNN-of-NNNNN.safetensors
            if name.starts_with("model-") && name.ends_with(".safetensors") && name.contains("-of-")
            {
                shards.push(path);
            }
        }
    }

    if shards.is_empty() {
        return Err(format!(
            "No safetensors files found in {}. Expected model.safetensors or model-NNNNN-of-NNNNN.safetensors",
            model_dir.display()
        ));
    }

    shards.sort();
    Ok(shards)
}

/// Compact a model's safetensors file according to the computed topology.
///
/// Reads the base model, slices attention tensors to remove pruned heads,
/// and writes the compacted model to `output_path`.
pub fn compact_model(
    model_path: &Path,
    topology: &HeadTopology,
    output_path: &Path,
) -> Result<CompactionResult, String> {
    let model_bytes = std::fs::read(model_path)
        .map_err(|e| format!("Failed to read model file {}: {}", model_path.display(), e))?;
    let original_size = model_bytes.len() as u64;

    let tensors = SafeTensors::deserialize(&model_bytes)
        .map_err(|e| format!("Failed to deserialize safetensors: {e}"))?;

    let mut output_tensors: HashMap<String, Vec<u8>> = HashMap::new();
    let mut output_metadata: HashMap<String, (Vec<usize>, Dtype)> = HashMap::new();

    for (name, tensor) in tensors.tensors() {
        let (data, shape, dtype) = compact_tensor(&name, &tensor, topology)?;
        output_metadata.insert(name.clone(), (shape, dtype));
        output_tensors.insert(name, data);
    }

    // Build the output safetensors
    let tensor_views: Vec<(String, Vec<usize>, Dtype, Vec<u8>)> = output_tensors
        .into_iter()
        .map(|(name, data)| {
            let (shape, dtype) = output_metadata.get(&name).unwrap().clone();
            (name, shape, dtype, data)
        })
        .collect();

    let tensor_refs: Vec<(&str, &[u8], &[usize], Dtype)> = tensor_views
        .iter()
        .map(|(name, shape, dtype, data)| {
            (name.as_str(), data.as_slice(), shape.as_slice(), *dtype)
        })
        .collect();

    let serialized = serialize_tensors(&tensor_refs)?;

    std::fs::write(output_path, &serialized).map_err(|e| {
        format!(
            "Failed to write compacted model to {}: {}",
            output_path.display(),
            e
        )
    })?;

    // Save topology alongside
    let topology_path = output_path.with_extension("topology.json");
    super::topology::save_topology(topology, &topology_path)?;

    Ok(CompactionResult {
        model_path: output_path.display().to_string(),
        topology_path: topology_path.display().to_string(),
        topology: topology.clone(),
        original_size_bytes: original_size,
        compacted_size_bytes: serialized.len() as u64,
    })
}

/// Compact a multi-shard model into a single output safetensors file.
///
/// This is the main entry point for 32B+ models that use multiple safetensor shards.
/// Reads all shards, compacts attention tensors, and writes a single output file.
///
/// For very large models (70B+), a streaming approach would be needed, but for
/// 32B models the compacted output fits in memory (~20-30GB → ~15-20GB compacted).
pub fn compact_model_sharded(
    model_dir: &Path,
    topology: &HeadTopology,
    output_path: &Path,
) -> Result<CompactionResult, String> {
    let shards = discover_shards(model_dir)?;

    eprintln!(
        "[compactor] Found {} shard(s) in {}",
        shards.len(),
        model_dir.display()
    );

    let mut all_output_tensors: Vec<(String, Vec<u8>, Vec<usize>, Dtype)> = Vec::new();
    let mut total_original_size: u64 = 0;

    for (shard_idx, shard_path) in shards.iter().enumerate() {
        eprintln!(
            "[compactor] Processing shard {}/{}: {}",
            shard_idx + 1,
            shards.len(),
            shard_path.display()
        );

        let shard_bytes = std::fs::read(shard_path)
            .map_err(|e| format!("Failed to read shard {}: {}", shard_path.display(), e))?;
        total_original_size += shard_bytes.len() as u64;

        let tensors = SafeTensors::deserialize(&shard_bytes)
            .map_err(|e| format!("Failed to deserialize shard {}: {e}", shard_path.display()))?;

        for (name, tensor) in tensors.tensors() {
            let (data, shape, dtype) = compact_tensor(&name, &tensor, topology)?;
            all_output_tensors.push((name, data, shape, dtype));
        }

        // Drop shard_bytes to free memory before loading next shard
        drop(shard_bytes);
    }

    eprintln!(
        "[compactor] Compacted {} tensors, serializing...",
        all_output_tensors.len()
    );

    // Serialize all tensors into a single output file
    let tensor_refs: Vec<(&str, &[u8], &[usize], Dtype)> = all_output_tensors
        .iter()
        .map(|(name, data, shape, dtype)| {
            (name.as_str(), data.as_slice(), shape.as_slice(), *dtype)
        })
        .collect();

    let serialized = serialize_tensors(&tensor_refs)?;

    std::fs::write(output_path, &serialized).map_err(|e| {
        format!(
            "Failed to write compacted model to {}: {}",
            output_path.display(),
            e
        )
    })?;

    // Save topology alongside
    let topology_path = output_path.with_extension("topology.json");
    super::topology::save_topology(topology, &topology_path)?;

    eprintln!(
        "[compactor] Done: {} → {} ({:.1}% reduction)",
        format_bytes(total_original_size),
        format_bytes(serialized.len() as u64),
        (1.0 - serialized.len() as f64 / total_original_size as f64) * 100.0
    );

    Ok(CompactionResult {
        model_path: output_path.display().to_string(),
        topology_path: topology_path.display().to_string(),
        topology: topology.clone(),
        original_size_bytes: total_original_size,
        compacted_size_bytes: serialized.len() as u64,
    })
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Compact a single tensor according to the topology.
/// Returns (data_bytes, new_shape, dtype).
fn compact_tensor(
    name: &str,
    tensor: &TensorView,
    topology: &HeadTopology,
) -> Result<(Vec<u8>, Vec<usize>, Dtype), String> {
    let shape = tensor.shape();
    let dtype = tensor.dtype();

    // Parse tensor name to find layer index and projection type
    // Pattern: "model.layers.{idx}.self_attn.{q|k|v|o}_proj.weight"
    if let Some((layer_idx, proj_type)) = parse_attention_tensor_name(name) {
        if let Some(layer_topo) = topology.layers.get(layer_idx) {
            let head_dim = topology.head_dim;
            return compact_attention_tensor(tensor, layer_topo, head_dim, &proj_type, dtype);
        }
    }

    // Non-attention tensor: copy verbatim
    Ok((tensor.data().to_vec(), shape.to_vec(), dtype))
}

/// Parse a tensor name to extract layer index and projection type.
///
/// Handles common patterns:
/// - "model.layers.0.self_attn.q_proj.weight"
/// - "model.layers.0.self_attn.q_proj.bias"
fn parse_attention_tensor_name(name: &str) -> Option<(usize, String)> {
    let parts: Vec<&str> = name.split('.').collect();

    // Find "layers" followed by index, then "self_attn"
    for i in 0..parts.len().saturating_sub(4) {
        if parts[i] == "layers" {
            if let Ok(layer_idx) = parts[i + 1].parse::<usize>() {
                if parts[i + 2] == "self_attn" {
                    let proj_name = parts[i + 3];
                    if matches!(proj_name, "q_proj" | "k_proj" | "v_proj" | "o_proj") {
                        // Include the weight/bias suffix
                        let suffix = if i + 4 < parts.len() {
                            parts[i + 4]
                        } else {
                            "weight"
                        };
                        return Some((layer_idx, format!("{proj_name}.{suffix}")));
                    }
                }
            }
        }
    }
    None
}

/// Compact an attention projection tensor by removing pruned head rows/columns.
fn compact_attention_tensor(
    tensor: &TensorView,
    layer: &LayerTopology,
    head_dim: usize,
    proj_type: &str,
    dtype: Dtype,
) -> Result<(Vec<u8>, Vec<usize>, Dtype), String> {
    let shape = tensor.shape();
    let data = tensor.data();
    let elem_size = dtype_size(dtype);

    match proj_type {
        // Q projection: slice ROWS by retained Q head indices
        // Shape: [num_heads * head_dim, hidden_size] → [retained_heads * head_dim, hidden_size]
        "q_proj.weight" => {
            if shape.len() != 2 {
                return Err(format!("q_proj.weight expected 2D, got {}D", shape.len()));
            }
            let hidden_size = shape[1];
            let new_rows = layer.num_heads * head_dim;
            let row_bytes = hidden_size * elem_size;
            let mut output = Vec::with_capacity(new_rows * row_bytes);

            for &head_idx in &layer.retained_head_indices {
                let row_start = head_idx * head_dim;
                for r in row_start..row_start + head_dim {
                    let offset = r * row_bytes;
                    let end = offset + row_bytes;
                    if end > data.len() {
                        return Err(format!("q_proj.weight: row {r} out of bounds"));
                    }
                    output.extend_from_slice(&data[offset..end]);
                }
            }

            Ok((output, vec![new_rows, hidden_size], dtype))
        }

        // K projection: slice ROWS by retained KV head indices
        "k_proj.weight" => {
            if shape.len() != 2 {
                return Err(format!("k_proj.weight expected 2D, got {}D", shape.len()));
            }
            let hidden_size = shape[1];
            let new_rows = layer.num_kv_heads * head_dim;
            let row_bytes = hidden_size * elem_size;
            let mut output = Vec::with_capacity(new_rows * row_bytes);

            for &kv_idx in &layer.retained_kv_head_indices {
                let row_start = kv_idx * head_dim;
                for r in row_start..row_start + head_dim {
                    let offset = r * row_bytes;
                    let end = offset + row_bytes;
                    if end > data.len() {
                        return Err(format!("k_proj.weight: row {r} out of bounds"));
                    }
                    output.extend_from_slice(&data[offset..end]);
                }
            }

            Ok((output, vec![new_rows, hidden_size], dtype))
        }

        // V projection: same slicing as K
        "v_proj.weight" => {
            if shape.len() != 2 {
                return Err(format!("v_proj.weight expected 2D, got {}D", shape.len()));
            }
            let hidden_size = shape[1];
            let new_rows = layer.num_kv_heads * head_dim;
            let row_bytes = hidden_size * elem_size;
            let mut output = Vec::with_capacity(new_rows * row_bytes);

            for &kv_idx in &layer.retained_kv_head_indices {
                let row_start = kv_idx * head_dim;
                for r in row_start..row_start + head_dim {
                    let offset = r * row_bytes;
                    let end = offset + row_bytes;
                    if end > data.len() {
                        return Err(format!("v_proj.weight: row {r} out of bounds"));
                    }
                    output.extend_from_slice(&data[offset..end]);
                }
            }

            Ok((output, vec![new_rows, hidden_size], dtype))
        }

        // O projection: slice COLUMNS by retained Q head indices
        // Shape: [hidden_size, num_heads * head_dim] → [hidden_size, retained_heads * head_dim]
        "o_proj.weight" => {
            if shape.len() != 2 {
                return Err(format!("o_proj.weight expected 2D, got {}D", shape.len()));
            }
            let rows = shape[0]; // hidden_size
            let original_cols = shape[1];
            let new_cols = layer.num_heads * head_dim;
            let mut output = Vec::with_capacity(rows * new_cols * elem_size);

            for row in 0..rows {
                let row_offset = row * original_cols * elem_size;
                for &head_idx in &layer.retained_head_indices {
                    let col_start = head_idx * head_dim;
                    for c in col_start..col_start + head_dim {
                        let byte_offset = row_offset + c * elem_size;
                        let end = byte_offset + elem_size;
                        if end > data.len() {
                            return Err(format!(
                                "o_proj.weight: element ({row}, {c}) out of bounds"
                            ));
                        }
                        output.extend_from_slice(&data[byte_offset..end]);
                    }
                }
            }

            Ok((output, vec![rows, new_cols], dtype))
        }

        // Bias vectors: slice by head indices
        "q_proj.bias" => {
            let new_len = layer.num_heads * head_dim;
            let mut output = Vec::with_capacity(new_len * elem_size);
            for &head_idx in &layer.retained_head_indices {
                let start = head_idx * head_dim * elem_size;
                let end = start + head_dim * elem_size;
                if end > data.len() {
                    return Err(format!("q_proj.bias: head {head_idx} out of bounds"));
                }
                output.extend_from_slice(&data[start..end]);
            }
            Ok((output, vec![new_len], dtype))
        }

        "k_proj.bias" | "v_proj.bias" => {
            let new_len = layer.num_kv_heads * head_dim;
            let mut output = Vec::with_capacity(new_len * elem_size);
            for &kv_idx in &layer.retained_kv_head_indices {
                let start = kv_idx * head_dim * elem_size;
                let end = start + head_dim * elem_size;
                if end > data.len() {
                    return Err(format!("{proj_type}: kv_head {kv_idx} out of bounds"));
                }
                output.extend_from_slice(&data[start..end]);
            }
            Ok((output, vec![new_len], dtype))
        }

        "o_proj.bias" => {
            // O projection bias is per hidden_size, NOT per head — copy verbatim
            Ok((data.to_vec(), shape.to_vec(), dtype))
        }

        // Unknown projection type: copy verbatim
        _ => Ok((data.to_vec(), shape.to_vec(), dtype)),
    }
}

/// Byte size per element for a given dtype.
fn dtype_size(dtype: Dtype) -> usize {
    match dtype {
        Dtype::F16 | Dtype::BF16 => 2,
        Dtype::F32 => 4,
        Dtype::F64 => 8,
        Dtype::I8 | Dtype::U8 | Dtype::BOOL => 1,
        Dtype::I16 | Dtype::U16 => 2,
        Dtype::I32 | Dtype::U32 => 4,
        Dtype::I64 | Dtype::U64 => 8,
        _ => 4, // Default fallback
    }
}

/// Serialize tensors to safetensors format.
fn serialize_tensors(tensors: &[(&str, &[u8], &[usize], Dtype)]) -> Result<Vec<u8>, String> {
    // Build tensor views for serialization
    let tensor_views: Vec<(String, TensorView<'_>)> = tensors
        .iter()
        .map(|(name, data, shape, dtype)| {
            let view = TensorView::new(*dtype, shape.to_vec(), data)
                .map_err(|e| format!("Failed to create TensorView for {name}: {e}"))
                .unwrap();
            (name.to_string(), view)
        })
        .collect();

    let refs: Vec<(&str, TensorView<'_>)> = tensor_views
        .iter()
        .map(|(name, view)| (name.as_str(), view.clone()))
        .collect();

    safetensors::tensor::serialize(refs, None)
        .map_err(|e| format!("Failed to serialize safetensors: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_attention_tensor_name_q_proj() {
        let result = parse_attention_tensor_name("model.layers.5.self_attn.q_proj.weight");
        assert_eq!(result, Some((5, "q_proj.weight".to_string())));
    }

    #[test]
    fn test_parse_attention_tensor_name_k_proj() {
        let result = parse_attention_tensor_name("model.layers.0.self_attn.k_proj.weight");
        assert_eq!(result, Some((0, "k_proj.weight".to_string())));
    }

    #[test]
    fn test_parse_attention_tensor_name_o_proj_bias() {
        let result = parse_attention_tensor_name("model.layers.12.self_attn.o_proj.bias");
        assert_eq!(result, Some((12, "o_proj.bias".to_string())));
    }

    #[test]
    fn test_parse_non_attention_tensor() {
        assert_eq!(
            parse_attention_tensor_name("model.layers.0.mlp.up_proj.weight"),
            None
        );
        assert_eq!(
            parse_attention_tensor_name("model.embed_tokens.weight"),
            None
        );
        assert_eq!(parse_attention_tensor_name("lm_head.weight"), None);
    }

    #[test]
    fn test_dtype_size() {
        assert_eq!(dtype_size(Dtype::BF16), 2);
        assert_eq!(dtype_size(Dtype::F16), 2);
        assert_eq!(dtype_size(Dtype::F32), 4);
        assert_eq!(dtype_size(Dtype::U8), 1);
    }

    #[test]
    fn test_compact_q_proj_removes_heads() {
        // Simulate a tiny q_proj: 4 heads, head_dim=2, hidden_size=4
        // Shape: [8, 4] (4 heads * 2 head_dim = 8 rows, 4 cols)
        // Each row is 4 f32 values = 16 bytes
        // Total: 8 * 16 = 128 bytes

        let head_dim = 2;
        let hidden_size = 4;
        let num_heads = 4;
        let rows = num_heads * head_dim; // 8

        // Create data: row i has all values = i as f32
        let mut data = Vec::with_capacity(rows * hidden_size * 4);
        for row in 0..rows {
            for _col in 0..hidden_size {
                data.extend_from_slice(&(row as f32).to_le_bytes());
            }
        }

        let shape = vec![rows, hidden_size];
        let view = TensorView::new(Dtype::F32, shape.clone(), &data).unwrap();

        // Retain heads 1 and 3 (remove heads 0 and 2)
        let layer = LayerTopology {
            layer_index: 0,
            num_heads: 2,
            num_kv_heads: 2,
            retained_head_indices: vec![1, 3],
            retained_kv_head_indices: vec![1, 3],
            head_precisions: vec![HeadPrecision::Q8, HeadPrecision::BF16],
            head_scores: vec![0.5, 0.8],
        };

        let (output, new_shape, dtype) =
            compact_attention_tensor(&view, &layer, head_dim, "q_proj.weight", Dtype::F32).unwrap();

        // New shape: [4, 4] (2 retained heads * 2 head_dim)
        assert_eq!(new_shape, vec![4, 4]);
        assert_eq!(dtype, Dtype::F32);

        // Verify we got rows 2,3 (head 1) and 6,7 (head 3)
        let floats: Vec<f32> = output
            .chunks(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        // Row 0 of output = original row 2 (head 1, offset 0)
        assert_eq!(floats[0], 2.0);
        assert_eq!(floats[1], 2.0);
        assert_eq!(floats[2], 2.0);
        assert_eq!(floats[3], 2.0);
        // Row 1 of output = original row 3 (head 1, offset 1)
        assert_eq!(floats[4], 3.0);
        // Row 2 of output = original row 6 (head 3, offset 0)
        assert_eq!(floats[8], 6.0);
        // Row 3 of output = original row 7 (head 3, offset 1)
        assert_eq!(floats[12], 7.0);
    }

    #[test]
    fn test_compact_o_proj_removes_columns() {
        // o_proj shape: [hidden_size, num_heads * head_dim] = [4, 8]
        // Slicing COLUMNS for retained heads
        let head_dim = 2;
        let hidden_size = 4;
        let num_heads = 4;
        let cols = num_heads * head_dim; // 8

        // Create data: element (row, col) = row * 10 + col as f32
        let mut data = Vec::with_capacity(hidden_size * cols * 4);
        for row in 0..hidden_size {
            for col in 0..cols {
                data.extend_from_slice(&((row * 10 + col) as f32).to_le_bytes());
            }
        }

        let shape = vec![hidden_size, cols];
        let view = TensorView::new(Dtype::F32, shape.clone(), &data).unwrap();

        // Retain heads 0 and 2 (columns 0,1 and 4,5)
        let layer = LayerTopology {
            layer_index: 0,
            num_heads: 2,
            num_kv_heads: 2,
            retained_head_indices: vec![0, 2],
            retained_kv_head_indices: vec![0, 2],
            head_precisions: vec![HeadPrecision::BF16, HeadPrecision::Q8],
            head_scores: vec![0.8, 0.5],
        };

        let (output, new_shape, _dtype) =
            compact_attention_tensor(&view, &layer, head_dim, "o_proj.weight", Dtype::F32).unwrap();

        // New shape: [4, 4] (hidden_size stays, cols = 2 heads * 2 head_dim)
        assert_eq!(new_shape, vec![4, 4]);

        let floats: Vec<f32> = output
            .chunks(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        // Row 0: should have columns 0, 1, 4, 5 from original
        assert_eq!(floats[0], 0.0); // (0, 0)
        assert_eq!(floats[1], 1.0); // (0, 1)
        assert_eq!(floats[2], 4.0); // (0, 4)
        assert_eq!(floats[3], 5.0); // (0, 5)

        // Row 1: columns 0, 1, 4, 5
        assert_eq!(floats[4], 10.0); // (1, 0)
        assert_eq!(floats[5], 11.0); // (1, 1)
        assert_eq!(floats[6], 14.0); // (1, 4)
        assert_eq!(floats[7], 15.0); // (1, 5)
    }

    #[test]
    fn test_discover_shards_single_file() {
        let dir = tempfile::tempdir().unwrap();
        let model_file = dir.path().join("model.safetensors");
        std::fs::write(&model_file, b"dummy").unwrap();

        let shards = discover_shards(dir.path()).unwrap();
        assert_eq!(shards.len(), 1);
        assert_eq!(shards[0], model_file);
    }

    #[test]
    fn test_discover_shards_multi() {
        let dir = tempfile::tempdir().unwrap();
        // Create fake shard files
        for i in 1..=3 {
            let name = format!("model-{:05}-of-00003.safetensors", i);
            std::fs::write(dir.path().join(name), b"dummy").unwrap();
        }

        let shards = discover_shards(dir.path()).unwrap();
        assert_eq!(shards.len(), 3);
        assert!(shards[0]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("00001"));
        assert!(shards[2]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("00003"));
    }

    #[test]
    fn test_discover_shards_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert!(discover_shards(dir.path()).is_err());
    }

    #[test]
    fn test_compact_identity_no_pruning() {
        // All heads retained → output should equal input
        let head_dim = 2;
        let hidden_size = 4;
        let num_heads = 2;

        let mut data = Vec::new();
        for i in 0..(num_heads * head_dim * hidden_size) {
            data.extend_from_slice(&(i as f32).to_le_bytes());
        }

        let shape = vec![num_heads * head_dim, hidden_size];
        let view = TensorView::new(Dtype::F32, shape.clone(), &data).unwrap();

        let layer = LayerTopology {
            layer_index: 0,
            num_heads: 2,
            num_kv_heads: 2,
            retained_head_indices: vec![0, 1],
            retained_kv_head_indices: vec![0, 1],
            head_precisions: vec![HeadPrecision::BF16, HeadPrecision::BF16],
            head_scores: vec![0.8, 0.8],
        };

        let (output, new_shape, _) =
            compact_attention_tensor(&view, &layer, head_dim, "q_proj.weight", Dtype::F32).unwrap();

        assert_eq!(new_shape, shape);
        assert_eq!(output, data);
    }
}
