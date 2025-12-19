/**
 * Archive Worker Configuration
 *
 * Self-contained worker definition - discovered by generator
 */

export default {
  name: 'archive',
  binary: 'workers/archive/target/release/archive-worker',
  socket: '/tmp/jtag-archive-worker.sock',
  args: [
    '/tmp/jtag-command-router.sock',
    '.continuum/jtag/data/database.sqlite',
    '.continuum/jtag/data/archive/database-001.sqlite'
  ],
  description: 'Archive worker for moving old data to cold storage using Commands.execute()',
  enabled: false
} as const;

export type ArchiveWorkerConfig = typeof import('./worker.config').default;
