/**
 * Archive Daemon Specification
 *
 * Low-CPU intermittent archiver that moves old data from active tables
 * to archive tables based on entity-level @Archive() configuration.
 */

import type { DaemonSpec } from '../DaemonTypes';

export const archiveDaemonSpec: DaemonSpec = {
  name: 'archive-daemon',
  description: 'Intermittent low-CPU archiver that manages entity archiving based on @Archive() decorators',

  jobs: [
    {
      name: 'checkAndArchive',
      description: 'Check all archivable entities and archive if needed',
      async: true,
      returns: 'void',
      params: []
    },
    {
      name: 'archiveEntity',
      description: 'Archive rows from a specific entity collection',
      async: true,
      returns: 'number', // Returns number of rows archived
      params: [
        { name: 'collection', type: 'string' },
        { name: 'maxRows', type: 'number' },
        { name: 'rowsPerArchive', type: 'number' }
      ]
    },
    {
      name: 'getArchiveStats',
      description: 'Get statistics about archived data',
      async: true,
      returns: 'Record<string, { activeRows: number; archivedRows: number }>',
      params: []
    }
  ],

  lifecycle: {
    onStart: 'Start the intermittent archive loop (5-10 minute intervals)',
    onStop: 'Stop the archive loop gracefully'
  }
};
