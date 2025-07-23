#!/usr/bin/env node
/**
 * Build-time Manifest Generator for JTAG System
 * 
 * Generates discovery manifests for both browser and server environments:
 * - daemon-manifest.ts: Maps daemon names to their import paths
 * - command-manifest.ts: Maps command names to their import paths per environment
 */

const fs = require('fs');
const path = require('path');

const JTAG_ROOT = __dirname;
const DAEMONS_DIR = path.join(JTAG_ROOT, 'daemons');
const MANIFESTS_DIR = path.join(JTAG_ROOT, 'manifests');

// Ensure manifests directory exists
if (!fs.existsSync(MANIFESTS_DIR)) {
  fs.mkdirSync(MANIFESTS_DIR, { recursive: true });
}

/**
 * Generate daemon manifest by scanning /daemons directory
 */
function generateDaemonManifest() {
  const daemonManifest = {
    browser: {},
    server: {}
  };

  // Scan daemons directory
  const daemonDirs = fs.readdirSync(DAEMONS_DIR, { withFileTypes: true })
    .filter(dirent => dirent.isDirectory())
    .map(dirent => dirent.name);

  for (const daemonName of daemonDirs) {
    const daemonPath = path.join(DAEMONS_DIR, daemonName);
    
    // Check for browser implementation
    const browserPath = path.join(daemonPath, 'browser');
    if (fs.existsSync(browserPath)) {
      const browserFiles = fs.readdirSync(browserPath)
        .filter(file => file.endsWith('.ts') && !file.endsWith('.test.ts'));
      
      if (browserFiles.length > 0) {
        const className = browserFiles[0].replace('.ts', '');
        const importPath = `../daemons/${daemonName}/browser/${className}`;
        daemonManifest.browser[toPascalCase(daemonName)] = {
          className,
          importPath
        };
      }
    }

    // Check for server implementation
    const serverPath = path.join(daemonPath, 'server');
    if (fs.existsSync(serverPath)) {
      const serverFiles = fs.readdirSync(serverPath)
        .filter(file => file.endsWith('.ts') && !file.endsWith('.test.ts'));
      
      if (serverFiles.length > 0) {
        const className = serverFiles[0].replace('.ts', '');
        const importPath = `../daemons/${daemonName}/server/${className}`;
        daemonManifest.server[toPascalCase(daemonName)] = {
          className,
          importPath
        };
      }
    }
  }

  // Generate TypeScript manifest file
  const manifestContent = `/**
 * Auto-generated Daemon Manifest
 * Generated at: ${new Date().toISOString()}
 * 
 * This file maps daemon names to their import paths for both browser and server environments.
 * Used for auto-discovery and dynamic loading of daemons.
 */

export interface DaemonManifestEntry {
  className: string;
  importPath: string;
}

export interface DaemonManifest {
  browser: Record<string, DaemonManifestEntry>;
  server: Record<string, DaemonManifestEntry>;
}

export const DAEMON_MANIFEST: DaemonManifest = ${JSON.stringify(daemonManifest, null, 2)};

/**
 * Get daemon manifest for specific environment
 */
export function getDaemonManifest(environment: 'browser' | 'server'): Record<string, DaemonManifestEntry> {
  return DAEMON_MANIFEST[environment];
}

/**
 * Get all daemon names for environment
 */
export function getDaemonNames(environment: 'browser' | 'server'): string[] {
  return Object.keys(DAEMON_MANIFEST[environment]);
}
`;

  fs.writeFileSync(path.join(MANIFESTS_DIR, 'daemon-manifest.ts'), manifestContent);
  console.log('✅ Generated daemon-manifest.ts');
}

/**
 * Generate command manifest by scanning command directories
 */
function generateCommandManifest() {
  const commandManifest = {
    browser: {},
    server: {}
  };

  // Find command daemon directory
  const commandDaemonPath = path.join(DAEMONS_DIR, 'command-daemon', 'commands');
  
  if (fs.existsSync(commandDaemonPath)) {
    const commandDirs = fs.readdirSync(commandDaemonPath, { withFileTypes: true })
      .filter(dirent => dirent.isDirectory())
      .map(dirent => dirent.name);

    for (const commandName of commandDirs) {
      const commandPath = path.join(commandDaemonPath, commandName);
      
      // Check for browser implementation
      const browserPath = path.join(commandPath, 'browser');
      if (fs.existsSync(browserPath)) {
        const browserFiles = fs.readdirSync(browserPath)
          .filter(file => file.endsWith('.ts') && !file.endsWith('.test.ts'));
        
        if (browserFiles.length > 0) {
          const className = browserFiles[0].replace('.ts', '');
          const importPath = `../daemons/command-daemon/commands/${commandName}/browser/${className}`;
          commandManifest.browser[commandName] = {
            className,
            importPath
          };
        }
      }

      // Check for server implementation
      const serverPath = path.join(commandPath, 'server');
      if (fs.existsSync(serverPath)) {
        const serverFiles = fs.readdirSync(serverPath)
          .filter(file => file.endsWith('.ts') && !file.endsWith('.test.ts'));
        
        if (serverFiles.length > 0) {
          const className = serverFiles[0].replace('.ts', '');
          const importPath = `../daemons/command-daemon/commands/${commandName}/server/${className}`;
          commandManifest.server[commandName] = {
            className,
            importPath
          };
        }
      }
    }
  }

  // Generate TypeScript manifest file
  const manifestContent = `/**
 * Auto-generated Command Manifest
 * Generated at: ${new Date().toISOString()}
 * 
 * This file maps command names to their import paths for both browser and server environments.
 * Used for auto-discovery and dynamic loading of commands.
 */

export interface CommandManifestEntry {
  className: string;
  importPath: string;
}

export interface CommandManifest {
  browser: Record<string, CommandManifestEntry>;
  server: Record<string, CommandManifestEntry>;
}

export const COMMAND_MANIFEST: CommandManifest = ${JSON.stringify(commandManifest, null, 2)};

/**
 * Get command manifest for specific environment
 */
export function getCommandManifest(environment: 'browser' | 'server'): Record<string, CommandManifestEntry> {
  return COMMAND_MANIFEST[environment];
}

/**
 * Get all command names for environment
 */
export function getCommandNames(environment: 'browser' | 'server'): string[] {
  return Object.keys(COMMAND_MANIFEST[environment]);
}
`;

  fs.writeFileSync(path.join(MANIFESTS_DIR, 'command-manifest.ts'), manifestContent);
  console.log('✅ Generated command-manifest.ts');
}

/**
 * Convert kebab-case to PascalCase
 */
function toPascalCase(str) {
  return str.replace(/(^\w|-\w)/g, (match) => 
    match.replace('-', '').toUpperCase()
  );
}

/**
 * Main execution
 */
function main() {
  console.log('🔨 Generating JTAG manifests...');
  
  try {
    generateDaemonManifest();
    generateCommandManifest();
    
    console.log('🎉 All manifests generated successfully!');
    console.log(`📁 Manifests saved to: ${MANIFESTS_DIR}`);
  } catch (error) {
    console.error('❌ Error generating manifests:', error);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main();
}

module.exports = { generateDaemonManifest, generateCommandManifest };