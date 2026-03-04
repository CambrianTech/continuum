/**
 * Archive Worker Configuration
 *
 * Self-contained worker definition - discovered by generator
 */

import { SystemPaths } from '../../system/core/config/SystemPaths';

export default {
  name: 'archive',
  binary: 'workers/archive/target/release/archive-worker',
  socket: '/tmp/jtag-archive-worker.sock',
  args: [
    '/tmp/jtag-command-router.sock',
    SystemPaths.database.main,
    `${SystemPaths.database.root}/archive/database-001.sqlite`
  ],
  description: 'Archive worker for moving old data to cold storage using Commands.execute()',
  enabled: true
} as const;

export type ArchiveWorkerConfig = typeof import('./worker.config').default;
