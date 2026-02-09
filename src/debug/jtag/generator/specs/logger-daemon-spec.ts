/**
 * Logger Daemon Specification
 *
 * Rust-backed daemon for high-performance logging.
 * Manages lifecycle of Rust logger worker connection.
 *
 * RUST-BACKED PATTERN:
 * - TypeScript: Connection management, health checks, lifecycle
 * - Rust worker: Heavy lifting (batching, file I/O, threading)
 * - Communication: Unix domain socket
 */

import type { DaemonSpec } from '../DaemonTypes';

export const loggerDaemonSpec: DaemonSpec = {
  name: 'logger-daemon',
  description: 'Rust-backed daemon for high-performance logging with Unix socket connection management',

  jobs: [
    {
      name: 'flush',
      description: 'Force flush all log buffers to disk',
      async: true,
      returns: 'void',
      params: []
    },
    {
      name: 'rotate',
      description: 'Rotate log files (close current, open new)',
      async: true,
      returns: 'void',
      params: [
        { name: 'category', type: 'string', optional: true }
      ]
    },
    {
      name: 'getStats',
      description: 'Get logging statistics from Rust worker',
      async: true,
      returns: 'Record<string, { messagesLogged: number; bytesWritten: number }>',
      params: []
    },
    {
      name: 'healthCheck',
      description: 'Check connection health to Rust worker',
      async: true,
      returns: 'boolean',
      params: []
    }
  ],

  lifecycle: {
    onStart: 'Connect to continuum-core LoggerModule via Unix socket (.continuum/sockets/continuum-core.sock)',
    onStop: 'Disconnect from continuum-core gracefully'
  }
};
