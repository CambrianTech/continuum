/**
 * TSX Configuration for Path Mapping Support
 * 
 * Makes tsx respect TypeScript path mappings from tsconfig.json
 * Critical for git hook validation and test runners
 */

import { readFileSync } from 'fs';
import { resolve } from 'path';

// Read tsconfig.json to extract path mappings
const tsconfigPath = resolve('./tsconfig.json');
const tsconfig = JSON.parse(readFileSync(tsconfigPath, 'utf-8'));

const paths = tsconfig.compilerOptions?.paths || {};
const baseUrl = tsconfig.compilerOptions?.baseUrl || '.';

// Convert TypeScript path mappings to tsx-compatible format
const pathMappings: Record<string, string> = {};

for (const [alias, targets] of Object.entries(paths)) {
  if (Array.isArray(targets) && targets.length > 0) {
    // Remove /* suffix from alias and target
    const cleanAlias = alias.replace(/\/\*$/, '');
    const cleanTarget = targets[0].replace(/\/\*$/, '');
    
    // Resolve relative to baseUrl
    pathMappings[cleanAlias] = resolve(baseUrl, cleanTarget);
  }
}

console.log('🔧 TSX: Loading path mappings:', pathMappings);

export default {
  // Enable path mapping resolution
  resolve: {
    alias: pathMappings
  },
  
  // Additional tsx options
  compilerOptions: {
    target: 'es2022',
    module: 'esnext',
    moduleResolution: 'node',
    allowSyntheticDefaultImports: true,
    esModuleInterop: true,
    skipLibCheck: true,
    strict: true
  }
};