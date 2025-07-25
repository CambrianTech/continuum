/**
 * Path Registration for tsx/Node.js
 * 
 * Registers TypeScript path mappings for runtime resolution
 * Used by git hooks and test runners to support clean imports
 */

import { readFileSync } from 'fs';
import { resolve, dirname } from 'path';
import { pathToFileURL } from 'url';
// @ts-ignore - Module augmentation for Node.js loader
import Module from 'module';

// Get the directory where tsconfig.json is located
const tsconfigPath = resolve('./tsconfig.json');
const tsconfigDir = dirname(tsconfigPath);

// Read and parse tsconfig.json
let tsconfig: any;
try {
  tsconfig = JSON.parse(readFileSync(tsconfigPath, 'utf-8'));
} catch (error) {
  console.warn('⚠️ Could not read tsconfig.json, path mappings disabled');
  process.exit(0);
}

const paths = tsconfig.compilerOptions?.paths || {};
const baseUrl = tsconfig.compilerOptions?.baseUrl || '.';
const baseUrlResolved = resolve(tsconfigDir, baseUrl);

console.log('🔧 Registering TypeScript path mappings...');

// Create alias mapping for module resolution
const aliases: Record<string, string> = {};

for (const [alias, targets] of Object.entries(paths)) {
  if (Array.isArray(targets) && targets.length > 0) {
    // Handle aliases with /* wildcard
    if (alias.endsWith('/*') && targets[0].endsWith('/*')) {
      const cleanAlias = alias.slice(0, -2); // Remove /*
      const cleanTarget = targets[0].slice(0, -2); // Remove /*
      const resolvedTarget = resolve(baseUrlResolved, cleanTarget);
      aliases[cleanAlias] = resolvedTarget;
      console.log(`  📁 ${cleanAlias}/* -> ${resolvedTarget}/*`);
    } else {
      // Handle exact aliases
      const resolvedTarget = resolve(baseUrlResolved, targets[0]);
      aliases[alias] = resolvedTarget;
      console.log(`  📄 ${alias} -> ${resolvedTarget}`);
    }
  }
}

// Hook into Node.js module resolution
const originalResolveFilename = Module._resolveFilename;

Module._resolveFilename = function (request: string, parent: any, isMain?: boolean) {
  // Check if this is an alias we should resolve
  for (const [alias, target] of Object.entries(aliases)) {
    if (request === alias) {
      // Exact match
      return originalResolveFilename.call(this, target, parent, isMain);
    } else if (request.startsWith(alias + '/')) {
      // Wildcard match
      const subpath = request.slice(alias.length + 1);
      const resolvedPath = resolve(target, subpath);
      return originalResolveFilename.call(this, resolvedPath, parent, isMain);
    }
  }
  
  // No alias match, use original resolution
  return originalResolveFilename.call(this, request, parent, isMain);
};

console.log('✅ Path mappings registered successfully');