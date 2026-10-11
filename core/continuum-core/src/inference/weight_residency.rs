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

    /// The KV (context) bytes alone on accelerators, compute buffers excluded.
    pub fn accelerator_context(&self) -> u64 {
        self.per_buffer.iter().filter(|b| !is_host_backend(&b.buffer_type)).map(|b| b.context_bytes).sum()
    }
}

/// The lane's KV on the device and the tokens it was allocated for, by the engine's own account:
/// the one per-token reading whose subject is this lane. A device delta also holds every other
/// tenant of the GPU and the engine's unreported overhead, and divided by tokens it charged them
/// to each served token (the 5090, build 6148, 2026-10-11: 85.8 KB/token by device delta, 46.4 KB
/// by this engine's own context bytes; card 03cdeee3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineContext {
    /// KV (context) bytes on accelerator buffers.
    pub context_bytes: u64,
    /// KV plus compute buffers on accelerators: everything the engine says it holds beyond weights.
    pub beyond_weights: u64,
    pub slots: u32,
    /// Each slot's context. The lane passes `--parallel` explicitly, so the cache is never unified
    /// and the engine holds `slots × n_ctx_per_slot` tokens.
    pub n_ctx_per_slot: u32,
}

/// What an answering engine's `/props` says about its context. Three answers, because only the
/// first two have a fallback (Codex on #4961): an engine that predates the field is measured by
/// the device delta, but one whose breakdown or slot geometry is present and unusable is no
/// reading at all. Feeding that to the delta would write the tenant-charged rate back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineContextReading {
    Read(EngineContext),
    /// No `memory_breakdown`: an engine older than fork #25.
    Absent,
    /// A breakdown is there, but it or the slot geometry does not parse, overflows, or is zero.
    Invalid,
}

impl EngineContextReading {
    pub fn of(props: &serde_json::Value) -> Self {
        if props.get("memory_breakdown").is_none() {
            return Self::Absent;
        }
        EngineContext::from_props(props).map_or(Self::Invalid, Self::Read)
    }
}

impl EngineContext {
    /// `None` when the engine predates `memory_breakdown`, or reports no slots, no context, or
    /// no KV on an accelerator. [`EngineContextReading::of`] tells those apart.
    pub fn from_props(props: &serde_json::Value) -> Option<Self> {
        let memory = EngineMemory::from_props(props)?;
        let slots = u32::try_from(props.get("total_slots")?.as_u64()?).ok()?;
        let n_ctx_per_slot = u32::try_from(props.pointer("/default_generation_settings/n_ctx")?.as_u64()?).ok()?;
        let context_bytes = memory.accelerator_context();
        (slots > 0 && n_ctx_per_slot > 0 && context_bytes > 0).then(|| Self {
            context_bytes,
            beyond_weights: memory.accelerator_beyond_weights(),
            slots,
            n_ctx_per_slot,
        })
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
        // and the per-token subject is the engine's KV over the tokens it holds (-np 2 → 2 × 2,048),
        // never compute or pinned host memory (card 03cdeee3)
        let mut with_slots = props.clone();
        with_slots["total_slots"] = 2.into();
        with_slots["default_generation_settings"] = serde_json::json!({"n_ctx": 2048});
        let ctx = EngineContext::from_props(&with_slots).expect("reads");
        assert_eq!((ctx.context_bytes, ctx.slots, ctx.n_ctx_per_slot), (117440512, 2, 2048));
        assert_eq!(ctx.beyond_weights, 117440512 + 66594944);
        assert_eq!(EngineContextReading::of(&with_slots), EngineContextReading::Read(ctx));
        // present but unusable is no reading, never the legacy fallback (Codex on #4961)
        assert_eq!(EngineContextReading::of(&props), EngineContextReading::Invalid, "no slot geometry");
        let mut zero_slots = with_slots.clone();
        zero_slots["total_slots"] = 0.into();
        assert_eq!(EngineContextReading::of(&zero_slots), EngineContextReading::Invalid);
        let mut overflow = with_slots.clone();
        overflow["default_generation_settings"]["n_ctx"] = (u64::from(u32::MAX) + 1).into();
        assert_eq!(EngineContextReading::of(&overflow), EngineContextReading::Invalid);
        let mut malformed = with_slots.clone();
        malformed["memory_breakdown"] = "not a list".into();
        assert_eq!(EngineContextReading::of(&malformed), EngineContextReading::Invalid);
        // only an engine without the field is the device delta's to measure
        assert_eq!(
            EngineContextReading::of(&serde_json::json!({"total_slots": 1, "default_generation_settings": {"n_ctx": 4096}})),
            EngineContextReading::Absent
        );
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
