/**
 * Logger Worker Configuration
 *
 * Self-contained worker definition - discovered by generator
 */

export default {
  name: 'logger',
  binary: 'workers/logger/target/release/logger-worker',
  socket: '/tmp/jtag-logger-worker.sock',
  description: 'High-performance logging worker for file I/O',
  enabled: true
} as const;

export type LoggerWorkerConfig = typeof import('./worker.config').default;
