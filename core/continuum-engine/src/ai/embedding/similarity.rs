//! Shared finite cosine scoring for embedding consumers.

// Shared SIMD kernel; exact-width chunks and a scalar tail cover every dimension.
fn simd_dot_and_norms(a: &[f32], b: &[f32]) -> (f32, f32, f32) {
    let mut dot = wide::f32x8::ZERO;
    let mut na = wide::f32x8::ZERO;
    let mut nb = wide::f32x8::ZERO;
    let (ca, tail_a) = a.as_chunks::<8>();
    let (cb, tail_b) = b.as_chunks::<8>();
    for (qa, qb) in ca.iter().zip(cb.iter()) {
        let va = wide::f32x8::new(*qa);
        let vb = wide::f32x8::new(*qb);
        dot = va.mul_add(vb, dot);
        na = va.mul_add(va, na);
        nb = vb.mul_add(vb, nb);
    }
    let mut d = dot.reduce_add();
    let mut sa = na.reduce_add();
    let mut sb = nb.reduce_add();
    for (x, y) in tail_a.iter().zip(tail_b.iter()) {
        d += x * y;
        sa += x * x;
        sb += y * y;
    }
    (d, sa, sb)
}

/// Cosine similarity in `[-1, 1]` (≈`[0,1]` for non-negative vectors). Returns
/// 0.0 for a length mismatch or a zero vector — a relevance score of "no signal",
/// never a panic.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (dot, na, nb) = simd_dot_and_norms(a, b);
    // Reject zero AND non-finite norms. A future EmbeddingProvider (neural/grid)
    // could return a NaN/Inf component; without this the `== 0.0` guard passes,
    // cosine returns NaN, and a NaN salience poisons the arbiter's sort
    // (every partial_cmp → Equal = arbitrary order). Fail to "no signal", never NaN.
    if !na.is_finite() || !nb.is_finite() || na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    let sim = dot / (na.sqrt() * nb.sqrt());
    if sim.is_finite() {
        sim
    } else {
        0.0
    }
}
