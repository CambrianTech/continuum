//! VQ3R/VQ2R record decode — the ONE backend-neutral reference both GPU
//! kernels implement (#231/#268 seam, the 1-10 tok/s lane).
//!
//! The foundry encodes (BigMama's `GgufRvqSource`); CUDA (her) and Metal
//! (me) decode at expert-gather time. Cross-backend drift here is the
//! exact bug class that produced fmt-0/1-vs-4/5 and the reuse=0 hunt, so
//! the contract is pinned three ways:
//!
//! 1. **The math, mined from upstream `tools/convert.py` (never guessed):**
//!    per-channel scale `s[m] = max|W[m,:]|.clamp(1e-8)` is applied BEFORE
//!    quantization; each stage is fitted on the running RESIDUAL; decode is
//!    `W[m, v*8..v*8+8] = s[m] * Σ_stages C_stage[idx][0..8]`.
//! 2. **The blocked index layout** (their measured 1.44× gather win):
//!    logical `[stages, M, nvr]` u8 indices are stored as
//!    `[M/64 blocks][nvr][64 rows][stages]`, M zero-padded to a multiple of
//!    IDX_BLOCK=64. One vector position's indices for a 64-row tile are
//!    contiguous. [`blocked_index_offset`] is the ONE offset formula.
//! 3. **Golden vectors** in the test mod: layout bytes pinned as literals +
//!    reconstruction checked against an independent direct computation.
//!    A GPU kernel passes when it reproduces the goldens byte/float-exact.
//!
//! Codebooks arrive as f32 rows (the loader converts the on-disk FP16 once,
//! at codebook load — [`f16_bytes_to_f32`]); the decoder itself is pure f32.

use half::f16;

/// Rows per index block — matches the engine's VQ_TILE and the packer's
/// IDX_BLOCK. Part of the container contract (manifest `expert_quant.
/// index_block` when we surface it); a mismatch decodes garbage, so the
/// decoder takes it as a parameter and the tests pin 64.
pub const IDX_BLOCK: usize = 64;
/// Vector dimension of every VQ stage (8-dim codebook entries).
pub const VEC_DIM: usize = 8;

/// Convert a little-endian FP16 byte slice (codebook data / per-channel
/// scales as stored on disk) into f32. The ONE conversion point.
pub fn f16_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|c| f16::from_le_bytes([c[0], c[1]]).to_f32())
        .collect()
}

/// Byte offset of the u8 index for (row `m`, vector position `v`, stage
/// `s`) inside a matrix's blocked index section. THE layout formula — the
/// gather kernels compute exactly this.
///
/// Layout: `[block][v][row_in_block][stage]` with `block = m / idx_block`,
/// `row_in_block = m % idx_block`, `nvr = N / VEC_DIM` vector positions.
pub fn blocked_index_offset(
    m: usize,
    v: usize,
    stage: usize,
    stages: usize,
    nvr: usize,
    idx_block: usize,
) -> usize {
    let block = m / idx_block;
    let row_in_block = m % idx_block;
    (((block * nvr + v) * idx_block + row_in_block) * stages) + stage
}

/// Bytes one matrix's blocked index section occupies: padded row count ×
/// nvr × stages. The reader uses this to walk gate→up→down sections.
pub fn blocked_index_section_bytes(m: usize, nvr: usize, stages: usize, idx_block: usize) -> usize {
    let padded_m = m.div_ceil(idx_block) * idx_block;
    padded_m * nvr * stages
}

/// Decode one full weight matrix from its blocked index section.
///
/// * `indices` — the matrix's blocked u8 index section (≥ the size given by
///   [`blocked_index_section_bytes`]).
/// * `codebooks` — one f32 slice per stage, each `256 × VEC_DIM` (entry-major
///   rows), already converted from disk FP16.
/// * `scale` — per-output-channel f32 scales (length `m_rows`), already
///   converted from the record's FP16 corrections section.
///
/// Returns the reconstructed `[m_rows × n_cols]` row-major f32 matrix.
/// Pure and allocation-simple by design: this is the CORRECTNESS reference
/// the GPU kernels are validated against, not the fast path.
pub fn decode_matrix(
    indices: &[u8],
    codebooks: &[&[f32]],
    scale: &[f32],
    m_rows: usize,
    n_cols: usize,
    idx_block: usize,
) -> Vec<f32> {
    let stages = codebooks.len();
    let nvr = n_cols / VEC_DIM;
    debug_assert_eq!(n_cols % VEC_DIM, 0, "N must be a multiple of VEC_DIM");
    debug_assert_eq!(scale.len(), m_rows, "one scale per output channel");
    debug_assert!(indices.len() >= blocked_index_section_bytes(m_rows, nvr, stages, idx_block));

    let mut out = vec![0.0f32; m_rows * n_cols];
    for m in 0..m_rows {
        let s = scale[m];
        for v in 0..nvr {
            let mut acc = [0.0f32; VEC_DIM];
            for (stage, cb) in codebooks.iter().enumerate() {
                let idx =
                    indices[blocked_index_offset(m, v, stage, stages, nvr, idx_block)] as usize;
                let row = &cb[idx * VEC_DIM..(idx + 1) * VEC_DIM];
                for d in 0..VEC_DIM {
                    acc[d] += row[d];
                }
            }
            let base = m * n_cols + v * VEC_DIM;
            for d in 0..VEC_DIM {
                out[base + d] = s * acc[d];
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-side reimplementation of the packer's `block_indices` (verbatim
    /// semantics from upstream convert.py): logical `[stages][M][nvr]` →
    /// blocked bytes. The packer must produce byte-identical output; the
    /// golden literal below pins that forever.
    fn pack_blocked(logical: &[Vec<Vec<u8>>], m: usize, nvr: usize, idx_block: usize) -> Vec<u8> {
        let stages = logical.len();
        let padded_m = m.div_ceil(idx_block) * idx_block;
        let mut out = vec![0u8; padded_m * nvr * stages];
        for (s, per_stage) in logical.iter().enumerate() {
            for (row, per_row) in per_stage.iter().enumerate() {
                for (v, &idx) in per_row.iter().enumerate() {
                    out[blocked_index_offset(row, v, s, stages, nvr, idx_block)] = idx;
                }
            }
        }
        out
    }

    // what this catches: THE cross-backend decode contract. Reconstruction
    // is checked against an INDEPENDENT direct computation (scale × Σ stage
    // rows), the blocked layout against a pinned golden byte literal, and
    // the padding path (M=3 padded to IDX_BLOCK) is exercised. A CUDA or
    // Metal kernel is correct when it reproduces exactly these numbers from
    // exactly these bytes.
    #[test]
    fn decode_matches_direct_computation_and_golden_layout() {
        let (m_rows, n_cols, stages) = (3usize, 16usize, 3usize);
        let nvr = n_cols / VEC_DIM; // 2

        // Deterministic codebooks: C_s[e][d] = (s+1) * (e as f32) + d/10.
        let cbs: Vec<Vec<f32>> = (0..stages)
            .map(|s| {
                (0..256)
                    .flat_map(|e| {
                        (0..VEC_DIM).map(move |d| (s + 1) as f32 * e as f32 + d as f32 * 0.1)
                    })
                    .collect()
            })
            .collect();
        let cb_refs: Vec<&[f32]> = cbs.iter().map(|c| c.as_slice()).collect();

        // Chosen indices [stages][M][nvr] and per-channel scales.
        let logical: Vec<Vec<Vec<u8>>> = vec![
            vec![vec![1, 2], vec![3, 4], vec![5, 6]],       // stage 0
            vec![vec![10, 20], vec![30, 40], vec![50, 60]], // stage 1
            vec![vec![7, 8], vec![9, 11], vec![13, 17]],    // stage 2
        ];
        let scale = [0.5f32, 2.0, 1.5];

        let blocked = pack_blocked(&logical, m_rows, nvr, IDX_BLOCK);
        assert_eq!(
            blocked.len(),
            blocked_index_section_bytes(m_rows, nvr, stages, IDX_BLOCK),
            "padded to a full IDX_BLOCK"
        );
        // GOLDEN: the first 18 bytes of the blocked section — block 0,
        // vector position 0, rows 0..2 (then zero padding), stages inner.
        // Layout = [block][v][row][stage] ⇒ row0:(1,10,7) row1:(3,30,9)
        // row2:(5,50,13) row3+:pad. Pinned so any packer/kernel drift on
        // ordering fails HERE, loudly, not as garbage weights downstream.
        assert_eq!(
            &blocked[..18],
            &[1, 10, 7, 3, 30, 9, 5, 50, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            "blocked index layout drifted from the pinned contract"
        );
        // Vector position 1 starts one padded tile later (64 rows × 3 stages).
        let v1 = IDX_BLOCK * stages;
        assert_eq!(&blocked[v1..v1 + 9], &[2, 20, 8, 4, 40, 11, 6, 60, 17]);

        let decoded = decode_matrix(&blocked, &cb_refs, &scale, m_rows, n_cols, IDX_BLOCK);

        // Independent direct computation, element by element.
        for m in 0..m_rows {
            for v in 0..nvr {
                for d in 0..VEC_DIM {
                    let expected: f32 = scale[m]
                        * (0..stages)
                            .map(|s| {
                                let e = logical[s][m][v] as usize;
                                cbs[s][e * VEC_DIM + d]
                            })
                            .sum::<f32>();
                    let got = decoded[m * n_cols + v * VEC_DIM + d];
                    assert!(
                        (got - expected).abs() < 1e-5,
                        "W[{m}][{}] = {got}, expected {expected}",
                        v * VEC_DIM + d
                    );
                }
            }
        }
    }

    // what this catches: the on-disk FP16 → f32 conversion point (codebook
    // data + correction scales share it). LE byte order and exact f16
    // semantics — a BE read or a bf16 confusion decodes every weight wrong.
    #[test]
    fn f16_conversion_is_le_and_exact_on_representable_values() {
        let vals = [0.5f32, -2.0, 1.5, 0.0];
        let bytes: Vec<u8> = vals
            .iter()
            .flat_map(|v| half::f16::from_f32(*v).to_le_bytes())
            .collect();
        assert_eq!(f16_bytes_to_f32(&bytes), vals);
    }
}
