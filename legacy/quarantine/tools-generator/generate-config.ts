/**
 * Configuration Generator
 *
 * Reads config.env and package.json at build time to generate shared/config.ts
 * This eliminates runtime configuration discovery and bundler issues.
 *
 * Pattern: Same as generate-version.ts - bake configuration into source at build time
 */

import { readFileSync } from 'fs';
import { writeIfChanged } from './core/writeIfChanged';
import { join } from 'path';

const rootDir = process.cwd();

// Inline `KEY=value` parser — replaces the previous `import * as dotenv
// from 'dotenv'` dependency. The layout sweep (PR #1557, task #214)
// moved this script from `src/generator/` to `tools/generator/`, which
// put it OUTSIDE the upward `node_modules` walk that resolves
// `src/node_modules/dotenv`. Node's resolution walks ancestors of the
// SCRIPT location, not `process.cwd()`, so `dotenv` was unfindable
// from `tools/generator/`. We only ever called `dotenv.parse()` (the
// pure string→KV transform), not the `.config()` side-effect path, so
// the inline parser is a like-for-like replacement at zero
// architectural cost. Bonus: generator scripts now have a node-stdlib-
// only footprint, matching `generate-version.ts`'s shape.
function parseEnvText(text: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eq = trimmed.indexOf('=');
    if (eq === -1) continue;
    const key = trimmed.slice(0, eq).trim();
    let value = trimmed.slice(eq + 1).trim();
    // Strip a single matched pair of surrounding quotes — matches
    // dotenv's behavior for `KEY="value"` and `KEY='value'`.
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    result[key] = value;
  }
  return result;
}

// Read config.env (follow SecretManager pattern for file locations)
function loadConfigEnv(): Record<string, string> {
  const configPaths = [
    join(process.env.HOME || '', '.continuum', 'config.env'),
    join(rootDir, 'config.env')
  ];

  let config: Record<string, string> = {};

  for (const configPath of configPaths) {
    try {
      const parsed = parseEnvText(readFileSync(configPath, 'utf-8'));
      config = { ...config, ...parsed };
    } catch {
      // File doesn't exist, continue
    }
  }

  return config;
}

// Read package.json
function loadPackageJson(path: string) {
  return JSON.parse(readFileSync(path, 'utf-8'));
}

// Generate the configuration
function generateConfig() {
  console.log('🔧 Generating shared/config.ts from config.env and package.json...');

  // Load config.env
  const configEnv = loadConfigEnv();
  const httpPort = parseInt(configEnv.HTTP_PORT || '9000');
  const wsPort = parseInt(configEnv.WS_PORT || '9001');

  if (!configEnv.HTTP_PORT || !configEnv.WS_PORT) {
    console.warn('⚠️  HTTP_PORT or WS_PORT not found in config.env, using defaults (9000, 9001)');
  }

  // Load main package.json
  const mainPackageJson = loadPackageJson(join(rootDir, 'package.json'));
  const activeExample = mainPackageJson.config?.active_example || 'test-bench';

  // Load example's package.json
  const exampleDir = join(rootDir, 'examples', activeExample);
  const examplePackageJson = loadPackageJson(join(exampleDir, 'package.json'));

  // Determine HTML file based on example
  const htmlFile = activeExample === 'widget-ui' ? 'index.html' : 'public/demo.html';

  // Generate TypeScript content
  // Note: socket paths resolve $HOME at RUNTIME (not build time) so the
  // generated file is portable across users. Browser-safe via typeof process guard.
  const content = `/**
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
// Browser connects to same host:port as page (widget-server proxies WS).
// These are server-side defaults only.
export const HTTP_PORT = ${httpPort};
export const WS_PORT = ${wsPort};

// Socket Configuration - Single Source of Truth
// $HOME resolved at runtime so the file is portable across users (any clone, any OS user).
// typeof guard keeps this safe when the module loads in a browser bundle.
const _HOME: string =
  (typeof process !== 'undefined' && process.env && (process.env.HOME || process.env.USERPROFILE)) || '';

// All Rust workers and TypeScript clients use these paths
export const SOCKET_DIR = \`\${_HOME}/.continuum/sockets\`;
export const SOCKETS = {
  /** Main continuum-core runtime socket */
  CONTINUUM_CORE: \`\${_HOME}/.continuum/sockets/continuum-core.sock\`,
  /** Archive worker socket */
  ARCHIVE: \`\${_HOME}/.continuum/sockets/archive-worker.sock\`,
  /** Inference/GPU worker socket (gRPC) */
  INFERENCE: \`\${_HOME}/.continuum/sockets/inference.sock\`,
} as const;

// Active Example Configuration (from package.json)
export const ACTIVE_EXAMPLE = '${activeExample}';

export const EXAMPLE_CONFIG = {
  name: '${examplePackageJson.name || `JTAG ${activeExample}`}',
  description: '${examplePackageJson.description || `${activeExample} development environment`}',
  ports: {
    http_server: HTTP_PORT,
    websocket_server: WS_PORT
  },
  paths: {
    directory: 'examples/${activeExample}',
    html_file: '${htmlFile}',
    build_output: 'dist'
  }
} as const;

// Type-safe exports matching ExampleDefinition
export type ExampleDefinition = typeof EXAMPLE_CONFIG;
`;

  // Write to shared/config.ts (only if changed)
  const outputPath = join(rootDir, 'shared', 'config.ts');
  const changed = writeIfChanged(outputPath, content);

  console.log(changed ? `✅ Generated shared/config.ts` : `⏭️  shared/config.ts unchanged`);
  console.log(`   HTTP_PORT: ${httpPort}`);
  console.log(`   WS_PORT: ${wsPort}`);
  console.log(`   ACTIVE_EXAMPLE: ${activeExample}`);
}

// Run generator
try {
  generateConfig();
} catch (error) {
  console.error('❌ Failed to generate config:', error);
  process.exit(1);
}
