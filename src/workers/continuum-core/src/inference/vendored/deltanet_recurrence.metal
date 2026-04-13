/// DeltaNet Fused Recurrence Kernel for Apple Metal
///
/// Replaces the per-timestep Rust loop with a single GPU dispatch.
/// Each threadgroup handles one (batch, head) pair.
/// Sequential timesteps within the kernel — recurrence is inherently sequential per head,
/// but all heads run in parallel across threadgroups.
///
/// Matches ggml_gated_delta_net op signature:
///   inputs:  q[S_k, H, T], k[S_k, H, T], v[S_v, H, T], g[H, T], beta[H, T], state[S_v, S_k, H]
///   outputs: out[S_v, H, T], state_out[S_v, S_k, H]

#include <metal_stdlib>
using namespace metal;

/// Single-token autoregressive path (generation hot path).
/// One token per head — no loop over T, just one state update + retrieval.
kernel void deltanet_recurrence_single(
    device const float* q       [[buffer(0)]],   // [S_k, H]
    device const float* k       [[buffer(1)]],   // [S_k, H]
    device const float* v       [[buffer(2)]],   // [S_v, H]
    device const float* g       [[buffer(3)]],   // [H] — decay gate (log space)
    device const float* beta    [[buffer(4)]],   // [H] — write gate
    device float*       state   [[buffer(5)]],   // [S_v, S_k, H] — in-place update
    device float*       output  [[buffer(6)]],   // [S_v, H]
    constant uint& S_k          [[buffer(7)]],
    constant uint& S_v          [[buffer(8)]],
    constant uint& H            [[buffer(9)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= H) return;

    uint h = tid;
    uint state_offset = h * S_v * S_k;
    uint q_offset = h * S_k;
    uint v_offset = h * S_v;

    // Decay: S *= exp(g)
    float decay = exp(g[h]);
    for (uint i = 0; i < S_v * S_k; i++) {
        state[state_offset + i] *= decay;
    }

    // Retrieve: out = S^T @ q
    for (uint sv = 0; sv < S_v; sv++) {
        float sum = 0.0f;
        for (uint sk = 0; sk < S_k; sk++) {
            sum += state[state_offset + sv * S_k + sk] * q[q_offset + sk];
        }
        output[v_offset + sv] = sum;
    }

    // Delta: delta = beta * (v - out)
    // Write: S += outer(k, delta)
    float beta_h = beta[h];
    for (uint sv = 0; sv < S_v; sv++) {
        float delta = beta_h * (v[v_offset + sv] - output[v_offset + sv]);
        for (uint sk = 0; sk < S_k; sk++) {
            state[state_offset + sv * S_k + sk] += k[q_offset + sk] * delta;
        }
    }

    // Re-read: out = S^T @ q (after write)
    for (uint sv = 0; sv < S_v; sv++) {
        float sum = 0.0f;
        for (uint sk = 0; sk < S_k; sk++) {
            sum += state[state_offset + sv * S_k + sk] * q[q_offset + sk];
        }
        output[v_offset + sv] = sum;
    }
}

/// Multi-token prefill path.
/// Sequential over T within each threadgroup, parallel across heads.
kernel void deltanet_recurrence_prefill(
    device const float* q       [[buffer(0)]],   // [S_k, H, T]
    device const float* k       [[buffer(1)]],   // [S_k, H, T]
    device const float* v       [[buffer(2)]],   // [S_v, H, T]
    device const float* g       [[buffer(3)]],   // [H, T] — decay gate
    device const float* beta    [[buffer(4)]],   // [H, T] — write gate
    device float*       state   [[buffer(5)]],   // [S_v, S_k, H] — in-place update
    device float*       output  [[buffer(6)]],   // [S_v, H, T]
    constant uint& S_k          [[buffer(7)]],
    constant uint& S_v          [[buffer(8)]],
    constant uint& H            [[buffer(9)]],
    constant uint& T            [[buffer(10)]],
    uint tid [[thread_position_in_grid]]
) {
    if (tid >= H) return;

    uint h = tid;
    uint state_offset = h * S_v * S_k;

    for (uint t = 0; t < T; t++) {
        uint qk_offset = (t * H + h) * S_k;
        uint v_offset  = (t * H + h) * S_v;
        uint g_offset  = t * H + h;
        uint out_offset = (t * H + h) * S_v;

        // Decay
        float decay = exp(g[g_offset]);
        for (uint i = 0; i < S_v * S_k; i++) {
            state[state_offset + i] *= decay;
        }

        // Retrieve: out = S^T @ q
        for (uint sv = 0; sv < S_v; sv++) {
            float sum = 0.0f;
            for (uint sk = 0; sk < S_k; sk++) {
                sum += state[state_offset + sv * S_k + sk] * q[qk_offset + sk];
            }
            output[out_offset + sv] = sum;
        }

        // Delta + Write
        float beta_t = beta[g_offset];
        for (uint sv = 0; sv < S_v; sv++) {
            float delta = beta_t * (v[v_offset + sv] - output[out_offset + sv]);
            for (uint sk = 0; sk < S_k; sk++) {
                state[state_offset + sv * S_k + sk] += k[qk_offset + sk] * delta;
            }
        }

        // Re-read after write
        for (uint sv = 0; sv < S_v; sv++) {
            float sum = 0.0f;
            for (uint sk = 0; sk < S_k; sk++) {
                sum += state[state_offset + sv * S_k + sk] * q[qk_offset + sk];
            }
            output[out_offset + sv] = sum;
        }
    }
}
