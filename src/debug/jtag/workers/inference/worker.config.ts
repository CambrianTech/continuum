/**
 * Inference Worker Configuration
 *
 * Candle-based LLM inference with multi-adapter LoRA composition.
 * Self-contained worker definition - discovered by generator.
 *
 * Key advantages over Ollama:
 * - Unix socket IPC (no HTTP overhead)
 * - Multi-adapter LoRA composition (genome vision)
 * - Metal acceleration on Apple Silicon
 * - In-process control (no external binary to manage)
 */

export default {
  name: 'inference',
  binary: 'workers/inference/target/release/inference-worker',
  socket: '/tmp/jtag-inference.sock',
  args: [
    '/tmp/jtag-inference.sock'  // Socket path passed as first arg
  ],
  description: 'Candle-based LLM inference with multi-adapter LoRA composition. Metal-accelerated.',
  enabled: true  // Worker is ready - verified with ping, model load, and generate
} as const;

export type InferenceWorkerConfig = typeof import('./worker.config').default;
