/**
 * LocalModelRouter — Routes tasks to the appropriate local model backend.
 *
 * GGUF Q5_K_S (default, 16GB machines):
 *   - Token-by-token prefill (~100ms/token on Metal, Metal SDPA bug)
 *   - Max tolerable system prompt: 350 tokens → ~35s prefill
 *   - Fits in 16GB with ~6GB headroom for KV cache
 *
 * BF16 safetensors (32GB machines, after dequantize_gguf one-time step):
 *   - Full-batch prefill in one forward pass (~2ms/token)
 *   - 150-token system prompt = ~0.3s vs 15s for GGUF
 *   - Max system prompt: 800 tokens — still fast
 *   - Detected by Rust: `bf16/` dir alongside GGUF + ≥24GB available RAM
 *
 * Threshold: totalVramMb > 28000 = 32GB machine (32768MB total - OS overhead)
 */

export interface RoutingDecision {
  /** Provider ID for the AI agent command */
  provider: string;
  /** Model alias passed to the Candle adapter (resolved server-side) */
  model: string;
  /** Whether this path supports full-batch prefill */
  usesBatchPrefill: boolean;
  /** Maximum system prompt tokens for this path */
  maxSystemTokens: number;
  /** Human-readable routing reason */
  reason: string;
}

import { LOCAL_MODELS } from '@system/shared/Constants';

export class LocalModelRouter {
  private static _instance: LocalModelRouter;

  static sharedInstance(): LocalModelRouter {
    if (!LocalModelRouter._instance) {
      LocalModelRouter._instance = new LocalModelRouter();
    }
    return LocalModelRouter._instance;
  }

  /**
   * Route to the appropriate local model based on total GPU memory.
   *
   * @param totalVramMb Total GPU memory from gpu/stats (0 = unknown → GGUF)
   */
  route(totalVramMb: number): RoutingDecision {
    if (totalVramMb > 28000) {
      return {
        provider: 'local',
        model: LOCAL_MODELS.CODING_AGENT_BF16,
        usesBatchPrefill: true,
        maxSystemTokens: 800,
        reason: `${totalVramMb}MB VRAM — BF16 safetensors, full-batch prefill`,
      };
    }

    return {
      provider: 'local',
      model: LOCAL_MODELS.CODING_AGENT,
      usesBatchPrefill: false,
      maxSystemTokens: 350,
      reason: `${totalVramMb}MB VRAM — GGUF Q5_K_S, token-by-token prefill`,
    };
  }
}
