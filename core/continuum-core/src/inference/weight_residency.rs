//! Weight residency — WHERE the model's weights were allocated, per backend, as
//! the engine reports it on `/props` (`model_weight_buffers`, fork commit
//! 3ca60da3c). A fact on a CHANNEL, not a console line (Joel, 2026-09-05:
//! "stdout is never a transport"). The offload banner and `n_gpu_layers` both
//! echo the REQUEST; this is the allocation.
//!
//! `None` from the parser means the engine predates the field — CHANNEL
//! UNAVAILABLE, a different fact from "on the CPU"; callers fall back to the
//! stderr arms and say so, never read absence as a placement.

/// Bytes of model weights per backend buffer type, e.g. `("Metal", 4_700_000_000)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightResidency {
    pub per_backend: Vec<(String, u64)>,
}

impl WeightResidency {
    /// Parse `/props`'s `model_weight_buffers`; `None` when the field is absent
    /// (older engine) or malformed.
    pub fn from_props(props: &serde_json::Value) -> Option<Self> {
        let arr = props.get("model_weight_buffers")?.as_array()?;
        let mut per_backend = Vec::with_capacity(arr.len());
        for b in arr {
            let name = b.get("backend")?.as_str()?.to_string();
            let bytes = b.get("size_bytes")?.as_u64()?;
            per_backend.push((name, bytes));
        }
        Some(Self { per_backend })
    }

    /// Bytes on an ACCELERATOR — every backend that is not host memory. Host-side
    /// buffer types: `CPU`, `CPU_Mapped`, `CPU_REPACK`, `*_Host` (CUDA pinned host
    /// memory is still RAM), and BLAS (Accelerate on a Mac runs on the CPU).
    pub fn accelerator_bytes(&self) -> u64 {
        self.per_backend
            .iter()
            .filter(|(name, _)| !is_host_backend(name))
            .map(|(_, b)| *b)
            .sum()
    }

    pub fn total_bytes(&self) -> u64 {
        self.per_backend.iter().map(|(_, b)| *b).sum()
    }

    /// One line for a probe or a refusal message: `Metal=4.7GB CPU_Mapped=0.3GB`.
    pub fn summary(&self) -> String {
        self.per_backend
            .iter()
            .map(|(n, b)| format!("{n}={:.2}GB", *b as f64 / 1e9))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// One buffer type's allocation in the serving context, as the engine reports it on `/props`
/// (`memory_breakdown`, fork #25): weights (model), KV cache (context), compute buffers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct BufferUse {
    pub buffer_type: String,
    pub model_bytes: u64,
    pub context_bytes: u64,
    pub compute_bytes: u64,
}

/// The serving context's whole footprint per buffer type, as the engine allocated it. The lane's
/// footprint where no process reading exists (Windows/WDDM attributes VRAM to no process) and an
/// adopted lane has no spawn baseline for a device delta (card 27fe9f8b).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct EngineMemory {
    #[serde(rename = "memory_breakdown")]
    pub per_buffer: Vec<BufferUse>,
}

impl EngineMemory {
    /// `None` when the engine predates the field (channel unavailable) or it does not parse.
    pub fn from_props(props: &serde_json::Value) -> Option<Self> {
        serde_json::from_value(props.clone()).ok()
    }

    /// The bytes BEYOND the weights on accelerators: KV cache + compute buffers on every buffer
    /// type that is not host memory. What the device delta measures on a discrete GPU.
    pub fn accelerator_beyond_weights(&self) -> u64 {
        self.per_buffer
            .iter()
            .filter(|b| !is_host_backend(&b.buffer_type))
            .map(|b| b.context_bytes + b.compute_bytes)
            .sum()
    }
}

fn is_host_backend(name: &str) -> bool {
    let n = name.trim();
    n.starts_with("CPU") || n.contains("_Host") || n.starts_with("BLAS")
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the channel's three answers — a GPU-resident model
    // (accelerator bytes = the Metal/CUDA buffers, host mapped bytes excluded),
    // a CPU-fallback model (accelerator bytes 0 though total is large), and an
    // engine without the field (None — channel unavailable, never "CPU").
    // what this catches: the lane's footprint read from the engine's own report — KV + compute
    // on accelerator buffers only (CUDA_Host is pinned RAM, CPU_Mapped holds mapped weights) —
    // and an engine without the field reading as unavailable, never as zero bytes.
    // Numbers: the 1.5B Q4_K_M at -c 4096 -np 2 on the 5090 (fork #25).
    #[test]
    fn engine_memory_counts_kv_and_compute_on_accelerators_only() {
        let props = serde_json::json!({"memory_breakdown": [
            {"buffer_type": "CUDA_Host", "model_bytes": 0u64, "context_bytes": 0u64, "compute_bytes": 8400928u64},
            {"buffer_type": "CPU_Mapped", "model_bytes": 191439360u64, "context_bytes": 0u64, "compute_bytes": 0u64},
            {"buffer_type": "CUDA0", "model_bytes": 980104704u64, "context_bytes": 117440512u64, "compute_bytes": 66594944u64}
        ]});
        let m = EngineMemory::from_props(&props).expect("parses");
        assert_eq!(m.accelerator_beyond_weights(), 117440512 + 66594944);
        assert!(EngineMemory::from_props(&serde_json::json!({"model_weight_buffers": []})).is_none());
    }

    #[test]
    fn residency_reads_allocation_and_absence_honestly() {
        let gpu = serde_json::json!({"model_weight_buffers": [
            {"backend": "CPU_Mapped", "size_bytes": 377487360u64},
            {"backend": "CUDA0", "size_bytes": 15_000_000_000u64},
            {"backend": "CUDA_Host", "size_bytes": 1024u64}
        ]});
        let r = WeightResidency::from_props(&gpu).unwrap();
        assert_eq!(r.accelerator_bytes(), 15_000_000_000);
        assert_eq!(r.total_bytes(), 15_000_000_000 + 377487360 + 1024);
        let cpu = serde_json::json!({"model_weight_buffers": [
            {"backend": "CPU_Mapped", "size_bytes": 4_034_000_000u64},
            {"backend": "BLAS", "size_bytes": 0u64}
        ]});
        assert_eq!(WeightResidency::from_props(&cpu).unwrap().accelerator_bytes(), 0);
        let old = serde_json::json!({"default_generation_settings": {"n_ctx": 4096}});
        assert_eq!(WeightResidency::from_props(&old), None);
        let metal = serde_json::json!({"model_weight_buffers": [
            {"backend": "Metal", "size_bytes": 4_700_000_000u64},
            {"backend": "CPU_Mapped", "size_bytes": 292_000_000u64}
        ]});
        let m = WeightResidency::from_props(&metal).unwrap();
        assert_eq!(m.accelerator_bytes(), 4_700_000_000);
        assert_eq!(m.summary(), "Metal=4.70GB CPU_Mapped=0.29GB");
    }
}
