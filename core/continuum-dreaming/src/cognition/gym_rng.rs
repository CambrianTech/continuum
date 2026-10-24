//! THE deterministic RNG for benchmark/gym selection — one LCG, everywhere.
//!
//! Replication is the whole point: `(seed, n, dataset)` must produce the SAME
//! selection on every machine, every OS, every build, forever. Platform `rand`
//! can't promise that across versions; this LCG can (Knuth MMIX constants).
//! One implementation (compression principle) — the vision-qa generator and
//! `benchmark/dispatch --sample` draw from THIS, never a private copy.

/// Tiny deterministic LCG — no rand dependency, byte-stable selections forever.
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Next value in `[0, bound)`. `bound` must be non-zero.
    pub fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the sequence changing ACROSS BUILDS — a published
    // (seed, n) pair must reproduce its instance list forever, so the first
    // draws from a pinned seed are pinned here as the compatibility contract.
    #[test]
    fn the_sequence_is_pinned_forever() {
        let mut r = Lcg::new(20260826);
        let draws: Vec<usize> = (0..5).map(|_| r.next(500)).collect();
        // LITERAL pin — the compatibility contract itself. If this assertion
        // ever fails, a published (seed, n) sample list has silently changed
        // meaning; fix the code, never the constants.
        assert_eq!(draws, vec![451, 288, 13, 9, 443], "seed 20260826 over a 500-row set");
    }
}
