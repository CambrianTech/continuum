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
