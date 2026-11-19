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
        let mut ctx = if only { model.new_pooled_context(params) } else { model.new_context(params) }.expect("context");
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
