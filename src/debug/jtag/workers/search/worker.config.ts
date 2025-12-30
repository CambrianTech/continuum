/**
 * Search Worker Configuration
 *
 * Self-contained worker definition - discovered by generator
 * Provides BoW, BM25, and future vector search algorithms via Unix socket
 */

export default {
  name: 'search',
  binary: 'workers/search/target/release/search-worker',
  socket: '/tmp/jtag-search-worker.sock',
  args: [],
  description: 'Search algorithms (BoW, BM25) off main thread via Unix socket',
  enabled: true
} as const;

export type SearchWorkerConfig = typeof import('./worker.config').default;
