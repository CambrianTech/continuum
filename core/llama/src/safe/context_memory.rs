//! Backend allocation plans produced by the same native context constructors as execution.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPlacement {
    Host,
    Device,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextAllocation {
    pub buffer: String,
    pub placement: MemoryPlacement,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ContextMemoryPlan {
    allocations: Vec<ContextAllocation>,
}

impl ContextMemoryPlan {
    pub fn allocations(&self) -> &[ContextAllocation] {
        &self.allocations
    }

    /// Validate the materialized buffers before executing a graph.
    pub fn covers(&self, actual: &Self) -> Result<(), String> {
        for row in &actual.allocations {
            let measured = self
                .allocations
                .iter()
                .filter(|a| a.buffer == row.buffer && a.placement == row.placement)
                .try_fold(0u64, |n, a| n.checked_add(a.bytes))
                .ok_or("context memory plan overflow")?;
            let allocated = actual
                .allocations
                .iter()
                .filter(|a| a.buffer == row.buffer && a.placement == row.placement)
                .try_fold(0u64, |n, a| n.checked_add(a.bytes))
                .ok_or("context allocation overflow")?;
            if allocated > measured {
                return Err(format!(
                    "context {} allocated {} bytes beyond measured {}",
                    row.buffer, allocated, measured
                ));
            }
        }
        Ok(())
    }
}

unsafe extern "C" fn collect(
    buffer: *const c_char,
    host: bool,
    bytes: u64,
    data: *mut std::ffi::c_void,
) {
    // The native visitor calls synchronously with a valid buffer name and this Vec.
    let rows = &mut *(data as *mut Vec<ContextAllocation>);
    if bytes != 0 {
        rows.push(ContextAllocation {
            buffer: std::ffi::CStr::from_ptr(buffer)
                .to_string_lossy()
                .into_owned(),
            placement: if host {
                MemoryPlacement::Host
            } else {
                MemoryPlacement::Device
            },
            bytes,
        });
    }
}

impl Model {
    /// Measures layouts and graph buffers against this loaded model's placement, without
    /// loading another set of weights or allocating KV/compute payloads.
    pub fn measure_context(&self, params: &ContextParams) -> Result<ContextMemoryPlan, String> {
        let mut allocations = Vec::<ContextAllocation>::new();
        let ok = unsafe {
            sys::llama_context_measure_memory(
                self.ptr.as_ptr(),
                params.to_ffi(),
                Some(collect),
                &mut allocations as *mut _ as *mut std::ffi::c_void,
            )
        };
        if !ok || allocations.is_empty() {
            return Err("native context memory measurement failed".into());
        }
        Ok(ContextMemoryPlan { allocations })
    }
}

impl Context<'_> {
    pub fn allocated_memory(&self) -> ContextMemoryPlan {
        let mut allocations = Vec::<ContextAllocation>::new();
        unsafe {
            sys::llama_context_visit_memory(
                self.ptr.as_ptr(),
                Some(collect),
                &mut allocations as *mut _ as *mut std::ffi::c_void,
            );
        }
        ContextMemoryPlan { allocations }
    }
}

impl ContextParams {
    pub(super) fn to_ffi(&self) -> sys::llama_context_params {
        let mut ffi = unsafe { sys::llama_context_default_params() };
        ffi.n_ctx = self.n_ctx;
        ffi.n_batch = self.n_batch;
        ffi.n_ubatch = self.n_ubatch;
        ffi.n_seq_max = self.n_seq_max;
        ffi.flash_attn_type = match self.flash_attn {
            FlashAttn::Auto => sys::llama_flash_attn_type_LLAMA_FLASH_ATTN_TYPE_AUTO,
            FlashAttn::Enabled => sys::llama_flash_attn_type_LLAMA_FLASH_ATTN_TYPE_ENABLED,
            FlashAttn::Disabled => sys::llama_flash_attn_type_LLAMA_FLASH_ATTN_TYPE_DISABLED,
        };
        ffi.fused_gdn_ar = self.fused_gdn_ar;
        ffi.fused_gdn_ch = self.fused_gdn_ch;
        ffi.type_k = match self.type_k {
            KvCacheType::F16 => sys::ggml_type_GGML_TYPE_F16,
            KvCacheType::Q8_0 => sys::ggml_type_GGML_TYPE_Q8_0,
        };
        ffi.type_v = match self.type_v {
            KvCacheType::F16 => sys::ggml_type_GGML_TYPE_F16,
            KvCacheType::Q8_0 => sys::ggml_type_GGML_TYPE_Q8_0,
        };
        ffi.embeddings = self.embeddings;
        ffi.pooling_type = match self.pooling_type {
            PoolingType::None => sys::llama_pooling_type_LLAMA_POOLING_TYPE_NONE,
            PoolingType::Mean => sys::llama_pooling_type_LLAMA_POOLING_TYPE_MEAN,
            PoolingType::Cls => sys::llama_pooling_type_LLAMA_POOLING_TYPE_CLS,
            PoolingType::Last => sys::llama_pooling_type_LLAMA_POOLING_TYPE_LAST,
        };

        ffi
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(rows: &[(MemoryPlacement, u64)]) -> ContextMemoryPlan {
        ContextMemoryPlan {
            allocations: rows
                .iter()
                .map(|&(placement, bytes)| ContextAllocation {
                    buffer: "same-buffer-name".into(),
                    placement,
                    bytes,
                })
                .collect(),
        }
    }

    #[test]
    fn mixed_placement_cannot_spend_host_credit_on_device() {
        use MemoryPlacement::*;
        let measured = plan(&[(Host, 100), (Device, 20)]);
        assert!(measured.covers(&plan(&[(Host, 90), (Device, 20)])).is_ok());
        assert!(measured.covers(&plan(&[(Host, 10), (Device, 30)])).is_err());
    }

    #[test]
    fn repeated_buffer_rows_are_counted_together() {
        use MemoryPlacement::*;
        let measured = plan(&[(Host, 20), (Host, 30)]);
        assert!(measured.covers(&plan(&[(Host, 50)])).is_ok());
        assert!(measured.covers(&plan(&[(Host, 25), (Host, 26)])).is_err());
        assert!(plan(&[(Host, u64::MAX), (Host, 1)])
            .covers(&plan(&[(Host, 1)]))
            .is_err());
    }

    #[test]
    fn native_measurement_rejects_missing_model() {
        let params = ContextParams::default();
        let mut rows = Vec::<ContextAllocation>::new();
        assert!(!unsafe {
            sys::llama_context_measure_memory(
                std::ptr::null_mut(),
                params.to_ffi(),
                Some(collect),
                &mut rows as *mut _ as *mut std::ffi::c_void,
            )
        });
        assert!(rows.is_empty());
    }

    #[test]
    #[ignore = "requires an explicit isolated CONTINUUM_TEST_EMBED_MODEL and backend"]
    fn native_plan_covers_embedding_materialization_and_decode() {
        let path = std::env::var_os("CONTINUUM_TEST_EMBED_MODEL").expect("explicit test model");
        let layers: i32 = std::env::var("CONTINUUM_TEST_GPU_LAYERS")
            .expect("explicit backend placement")
            .parse()
            .expect("integer layer count");
        let model = Model::load(
            path,
            ModelParams {
                n_gpu_layers: layers,
                use_mmap: true,
            },
        )
        .expect("load isolated embedding model");
        let tokens = model
            .tokenize(
                "A corrected implementation preserves its owner's work.",
                true,
                false,
            )
            .expect("tokenize");
        for n_ctx in [tokens.len() as u32, 256, 512] {
            let params = ContextParams {
                n_ctx,
                n_batch: n_ctx,
                n_ubatch: n_ctx,
                embeddings: true,
                pooling_type: PoolingType::Last,
                ..Default::default()
            };
            let plan = model
                .measure_context(&params)
                .expect("measure on loaded model");
            eprintln!("layers={layers} n_ctx={n_ctx} plan={plan:?}");
            let mut context = model.new_context(params).expect("materialize");
            plan.covers(&context.allocated_memory())
                .expect("initial allocation covered");
            let vector = context.embed(&tokens).expect("actual decode");
            assert!(vector.iter().all(|x| x.is_finite()));
            plan.covers(&context.allocated_memory())
                .expect("decode allocations covered");
        }
    }
}
