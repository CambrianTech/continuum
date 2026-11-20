use candle_core::quantized::gguf_file::Content;

/// Validated split declaration. An absent declaration is an ordinary GGUF;
/// a present declaration must carry every field with consistent geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GgufSplit {
    pub count: u16,
    pub index: u16,
    pub tensors: u64,
}

pub fn split(ct: &Content) -> Result<Option<GgufSplit>, String> {
    let Some(count) = ct.metadata.get("split.count") else {
        if ct.metadata.contains_key("split.no") || ct.metadata.contains_key("split.tensors.count") {
            return Err("GGUF split fields without split.count".into());
        }
        return Ok(None);
    };
    let count = count.to_u16().map_err(|e| e.to_string())?;
    let index = ct
        .metadata
        .get("split.no")
        .ok_or("missing split.no")?
        .to_u16()
        .map_err(|e| e.to_string())?;
    let tensors = ct
        .metadata
        .get("split.tensors.count")
        .ok_or("missing split.tensors.count")?
        .to_i32()
        .map_err(|e| e.to_string())?;
    let tensors = u64::try_from(tensors).map_err(|_| "negative split.tensors.count")?;
    if count == 0 || index >= count {
        return Err("invalid GGUF split index/count".into());
    }
    Ok(Some(GgufSplit {
        count,
        index,
        tensors,
    }))
}

