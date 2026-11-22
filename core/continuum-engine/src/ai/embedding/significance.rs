//! When a similarity is meaningful in an embedding space: the one owner of that judgment. A neural
//! space is anisotropic (unrelated texts do not score near 0: Qwen3-Embedding's measured unrelated null
//! on the 5090 was 0.30477 ± 0.08683, 2026-11-22), so every consumer judges a cosine against the space's
//! MEASURED null (`EmbeddingProvider::unrelated_null`), never against an absolute number. Recall's focus
//! policy and a gene signature's themes both read this, so the two never disagree about what is related.

/// The significance bar, in standard deviations above the space's unrelated null: 3σ, the statistical
/// convention recall shipped with (`SignificanceRanker`), not an embedder constant.
pub const SIGNIFICANCE_SIGMA: f32 = 3.0;

/// The cosine at `sigma` standard deviations above the space's measured unrelated null `(mean, std)`.
pub fn significance_floor((mean, std): (f32, f32), sigma: f32) -> f32 {
    mean + sigma * std
}

/// The share of unrelated pairs that score at or above `sigma` standard deviations over the null: the
/// normal upper tail `1 - Φ(sigma)`, by Abramowitz & Stegun 7.1.26 (absolute error below 1.5e-7, far
/// under anything a count of examples can see). What chance alone puts above a significance floor.
pub fn chance_above(sigma: f32) -> f32 {
    let x = (sigma / std::f32::consts::SQRT_2).abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t * (0.254_829_6 + t * (-0.284_496_74 + t * (1.421_413_7 + t * (-1.453_152_1 + t * 1.061_405_4))));
    let erfc = poly * (-x * x).exp();
    if sigma >= 0.0 {
        0.5 * erfc
    } else {
        1.0 - 0.5 * erfc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a floor read off the wrong side of the null, or a tail that is not the
    // normal one (the chance count a theme must beat would then admit noise or refuse real themes).
    // Reference values: 1 - Φ(0) = 0.5, 1 - Φ(1) = 0.158655, 1 - Φ(3) = 0.0013499.
    #[test]
    fn the_floor_sits_above_the_null_and_the_tail_is_the_normal_one() {
        assert!((significance_floor((0.30477, 0.08683), SIGNIFICANCE_SIGMA) - 0.56526).abs() < 1e-4);
        assert!((chance_above(0.0) - 0.5).abs() < 1e-6);
        assert!((chance_above(1.0) - 0.158_655).abs() < 1e-6);
        assert!((chance_above(3.0) - 0.001_349_9).abs() < 1e-6);
        assert!((chance_above(-1.0) - 0.841_345).abs() < 1e-6);
    }
}
