use super::*;

#[test]
fn pooled_only_preserves_embedding_and_omits_logits() {
    let path = std::env::var_os("CONTINUUM_TEST_EMBED_MODEL").expect("fixture model");
    let model = Model::load(path, ModelParams { n_gpu_layers: 0, use_mmap: true }).expect("load");
    let tokens = model.tokenize(&"The resident preserves her work. ".repeat(128), true, false).expect("tokenize");
    let width = u32::try_from(tokens.len()).expect("fixture width");
    let mut outputs = Vec::new();
    for only in [false, false, true, true] {
        let params = ContextParams {
            embeddings: true, pooling_type: PoolingType::Last,
            n_ctx: width, n_batch: width, n_ubatch: width, ..Default::default()
        };
        let mut ctx = if only { model.new_pooled_context(params).expect("pooled context") } else { model.new_context(params).expect("normal context") };
        outputs.push(ctx.embed(&tokens).expect("whole sequence"));
        // SAFETY: ctx owns a live context. The returned pointer is only checked for null.
        assert_eq!(unsafe { sys::llama_get_logits(ctx.ptr.as_ptr()) }.is_null(), only);
    }
    for (run, output) in outputs.iter().enumerate().skip(1) {
        assert_eq!(outputs[0].len(), output.len());
        let max_delta = outputs[0].iter().zip(output)
            .map(|(before, after)| (before - after).abs()).fold(0.0_f32, f32::max);
        eprintln!("embedding run {run}: max absolute delta {max_delta}");
        assert!(output.iter().all(|value| value.is_finite()));
        assert!(max_delta < 1e-5, "embedding changed in run {run}: {max_delta}");
    }
}

#[test]
fn pooled_embedding_preserves_text_after_interior_nul() {
    let path = std::env::var_os("CONTINUUM_TEST_EMBED_MODEL").expect("fixture model");
    let model = Model::load(path, ModelParams { n_gpu_layers: 0, use_mmap: true }).expect("load");
    let text = "A".repeat(7135) + "\0different suffix";
    let tokens = model.tokenize(&text, false, false).expect("tokenize full input");
    let decoded: String = tokens.iter().map(|token| model.token_to_piece(*token)).collect();
    assert_eq!(decoded, text);

    let width = u32::try_from(tokens.len()).expect("fixture width");
    let mut ctx = model.new_pooled_context(ContextParams {
        n_ctx: width, n_batch: width, n_ubatch: width,
        pooling_type: PoolingType::Last, ..Default::default()
    }).expect("pooled context");
    let output = ctx.embed(&tokens).expect("embed full input");
    assert!(!output.is_empty() && output.iter().all(|v| v.is_finite()));
}

#[test]
fn pooled_quote_matches_allocated_buffers() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from(std::env::var("CONTINUUM_TEST_EMBED_MODEL")?);
    let planner = PooledContextPlanner::open(&path, 0)?;
    let model = Model::load(&path, ModelParams { n_gpu_layers: 0, use_mmap: true })?;
    for width in [256, 2048] {
        let params = || ContextParams {
            pooling_type: PoolingType::Last,
            n_ctx: width,
            n_batch: width,
            n_ubatch: width,
            ..Default::default()
        };
        let projected = planner.quote(params())?;
        let allocated = model.new_pooled_context(params())?;
        let mut bytes = sys::llama_context_buffer_bytes { host: 0, device: 0 };
        // SAFETY: the allocated context and output buffer live through the call.
        assert!(unsafe { sys::llama_context_get_buffer_bytes(allocated.ptr.as_ptr(), &mut bytes) });
        assert!(bytes.host + bytes.device > 0);
        assert_eq!((projected.host, projected.device), (bytes.host, bytes.device), "width {width}");
    }
    Ok(())
}
