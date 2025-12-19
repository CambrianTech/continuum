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
    '.continuum/jtag/data/database.sqlite'
  ],
  description: 'Data daemon worker for WAL cleanup and fast SQLite operations',
  enabled: true  // PRODUCTION READY - tested ping, open, create, read
} as const;

export type DataDaemonWorkerConfig = typeof import('./worker.config').default;
