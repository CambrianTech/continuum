//! Deterministic PRNG for animation systems.
//!
//! `SlotRng` uses xorshift32 — uniform [0,1) distribution, no sin(),
//! no floating-point precision loss. Safe at any elapsed time magnitude.

/// Deterministic per-slot random number generator.
///
/// Seeds from elapsed time bits + slot index using bit-mixing.
/// Each call to `next()` advances the internal state.
pub(in crate::live::video::bevy_renderer) struct SlotRng {
    state: u32,
}

impl SlotRng {
    /// Create a new SlotRng seeded from elapsed seconds and slot index.
    #[inline]
    pub fn new(elapsed_secs: f32, slot: u8) -> Self {
        let mut h = elapsed_secs.to_bits();
        h ^= (slot as u32).wrapping_mul(0x9E3779B9);
        h ^= h >> 16;
        h = h.wrapping_mul(0x45D9F3B);
        h ^= h >> 16;
        // Ensure state is never 0 (xorshift32 fixpoint)
        if h == 0 {
            h = 0xDEADBEEF;
        }
        Self { state: h }
    }

    /// Generate next uniform f32 in [0, 1).
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        (self.state >> 8) as f32 / 16777216.0 // 2^24
    }

    /// Generate next f32 in [min, max).
    #[inline]
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_in_range() {
        for slot in 0..16u8 {
            let mut rng = SlotRng::new(42.0, slot);
            for _ in 0..1000 {
                let v = rng.next_f32();
                assert!(v >= 0.0 && v < 1.0, "out of range: {v}");
            }
        }
    }

    #[test]
    fn uniform_distribution() {
        let mut buckets = [0u32; 10];
        let mut rng = SlotRng::new(1.0, 0);
        for _ in 0..10000 {
            let v = rng.next_f32();
            buckets[(v * 10.0) as usize % 10] += 1;
        }
        for (i, &count) in buckets.iter().enumerate() {
            assert!(
                count > 700 && count < 1300,
                "Bucket {i} has {count} entries (expected ~1000)"
            );
        }
    }

    #[test]
    fn different_slots_different_sequences() {
        let mut rng_a = SlotRng::new(100.0, 0);
        let mut rng_b = SlotRng::new(100.0, 1);
        let a: Vec<f32> = (0..10).map(|_| rng_a.next_f32()).collect();
        let b: Vec<f32> = (0..10).map(|_| rng_b.next_f32()).collect();
        assert_ne!(a, b, "Different slots must produce different sequences");
    }

    #[test]
    fn large_elapsed_times_still_work() {
        // This is the bug that sin().abs() had — large elapsed_secs caused degeneration
        for elapsed in [600.0, 3600.0, 86400.0, 1_000_000.0] {
            let mut rng = SlotRng::new(elapsed, 5);
            let v = rng.next_f32();
            assert!(v >= 0.0 && v < 1.0, "Failed at elapsed={elapsed}: {v}");
        }
    }

    #[test]
    fn range_respects_bounds() {
        let mut rng = SlotRng::new(7.0, 3);
        for _ in 0..1000 {
            let v = rng.range(2.0, 5.0);
            assert!(v >= 2.0 && v < 5.0, "range out of bounds: {v}");
        }
    }
}
