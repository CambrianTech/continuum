/**
 * Configuration Generator
 *
 * Reads config.env and package.json at build time to generate shared/config.ts
 * This eliminates runtime configuration discovery and bundler issues.
 *
 * Pattern: Same as generate-version.ts - bake configuration into source at build time
 */

import { readFileSync, writeFileSync } from 'fs';
import { join } from 'path';
import * as dotenv from 'dotenv';

const rootDir = process.cwd();

// Read config.env (follow SecretManager pattern for file locations)
function loadConfigEnv(): Record<string, string> {
  const configPaths = [
    join(process.env.HOME || '', '.continuum', 'config.env'),
    join(rootDir, 'config.env')
  ];

  let config: Record<string, string> = {};

  for (const configPath of configPaths) {
    try {
      const parsed = dotenv.parse(readFileSync(configPath, 'utf-8'));
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

  // Socket configuration - single source of truth
  // Use .continuum/sockets/ for proper isolation from system /tmp
  const socketDir = '.continuum/sockets';

  // Generate TypeScript content
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
export const HTTP_PORT = ${httpPort};
export const WS_PORT = ${wsPort};

// Socket Configuration - Single Source of Truth
// All Rust workers and TypeScript clients use these paths
export const SOCKET_DIR = '${socketDir}';
export const SOCKETS = {
  /** Main continuum-core runtime socket */
  CONTINUUM_CORE: '${socketDir}/continuum-core.sock',
  /** Archive worker socket */
  ARCHIVE: '${socketDir}/archive-worker.sock',
  /** Inference/GPU worker socket (gRPC) */
  INFERENCE: '${socketDir}/inference.sock',
} as const;

// Active Example Configuration (from package.json)
export const ACTIVE_EXAMPLE = '${activeExample}';

export const EXAMPLE_CONFIG = {
  name: '${examplePackageJson.name || `JTAG ${activeExample}`}',
  description: '${examplePackageJson.description || `${activeExample} development environment`}',
  ports: {
    http_server: ${httpPort},
    websocket_server: ${wsPort}
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

  // Write to shared/config.ts
  const outputPath = join(rootDir, 'shared', 'config.ts');
  writeFileSync(outputPath, content, 'utf-8');

  console.log(`✅ Generated shared/config.ts`);
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
