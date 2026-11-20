use candle_core::quantized::gguf_file::Content;

/// A declared model total or the element count of a complete tensor table.
/// File length includes metadata, padding and mixed quantization; it is not a
/// parameter count and must not become residency evidence.
pub fn artifact_parameter_count(content: &Content) -> Result<u64, String> {
    if let Some(value) = content.metadata.get("general.parameter_count") {
        let count = value.to_u64().map_err(|e| format!("invalid parameter count: {e}"))?;
        return if count > 0 { Ok(count) } else { Err("zero parameter count".into()) };
    }
    if super::split(content)?.is_some_and(|split| split.count > 1) {
        return Err("partial GGUF shard cannot establish the model parameter total".into());
    }
    let mut count = 0_u64;
    for tensor in content.tensor_infos.values() {
        let mut elements = 1_u64;
        for &dimension in tensor.shape.dims() {
            let dimension = u64::try_from(dimension).map_err(|e| e.to_string())?;
            elements = elements.checked_mul(dimension).ok_or("tensor element count overflow")?;
        }
        if elements == 0 {
            return Err("empty tensor cannot establish parameter count".into());
        }
        count = count.checked_add(elements).ok_or("model parameter count overflow")?;
    }
    if count == 0 {
        return Err("empty tensor table cannot establish parameter count".into());
    }
    Ok(count)
}

