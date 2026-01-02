/**
 * Embedding Worker Configuration
 *
 * Self-contained worker definition - discovered by generator
 */

export default {
  name: 'embedding',
  binary: 'workers/embedding/target/release/embedding-worker',
  socket: '/tmp/jtag-embedding.sock',
  args: [
    '/tmp/jtag-embedding.sock'  // Socket path passed as first arg
  ],
  description: 'Native embedding generation via fastembed (ONNX). ~5ms vs ~80ms Ollama HTTP.',
  enabled: true
} as const;

export type EmbeddingWorkerConfig = typeof import('./worker.config').default;
