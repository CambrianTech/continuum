/**
 * Configuration Constants - Auto-generated at Build Time
 *
 * Generated from:
 * - config.env (ports)
 * - package.json (active example)
 * - examples/[example]/package.json (example metadata)
 *
 * DO NOT EDIT MANUALLY - Changes will be overwritten
 * Edit source files and run: npm run build
 */

// Network Configuration (from config.env)
export const HTTP_PORT = 9000;
export const WS_PORT = 9001;

// Socket Configuration - Single Source of Truth
// All Rust workers and TypeScript clients use these paths
export const SOCKET_DIR = '.continuum/sockets';
export const SOCKETS = {
  /** Main continuum-core runtime socket */
  CONTINUUM_CORE: '.continuum/sockets/continuum-core.sock',
  /** Archive worker socket */
  ARCHIVE: '.continuum/sockets/archive-worker.sock',
  /** Inference/GPU worker socket (gRPC) */
  INFERENCE: '.continuum/sockets/inference.sock',
} as const;

// Active Example Configuration (from package.json)
export const ACTIVE_EXAMPLE = 'widget-ui';

export const EXAMPLE_CONFIG = {
  name: '@continuum/jtag-widget-ui',
  description: 'JTAG Widget UI - Positron Widget System (Vite + Signals)',
  ports: {
    http_server: 9000,
    websocket_server: 9001
  },
  paths: {
    directory: 'examples/widget-ui',
    html_file: 'index.html',
    build_output: 'dist'
  }
} as const;

// Type-safe exports matching ExampleDefinition
export type ExampleDefinition = typeof EXAMPLE_CONFIG;
