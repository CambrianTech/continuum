/**
 * Data Daemon Worker Configuration
 *
 * Self-contained worker definition - discovered by generator
 */

export default {
  name: 'data-daemon',
  binary: 'workers/data-daemon/target/release/data-daemon-worker',
  socket: '/tmp/jtag-data-daemon-worker.sock',
  args: [
    '/tmp/jtag-data-daemon-worker.sock'  // Socket path passed as first arg
  ],
  description: 'Data daemon worker for WAL cleanup and fast SQLite operations',
  enabled: true
} as const;

export type DataDaemonWorkerConfig = typeof import('./worker.config').default;
