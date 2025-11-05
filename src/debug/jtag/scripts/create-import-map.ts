#!/usr/bin/env npx tsx

/**
 * Creates Node.js import map for path aliases
 * Uses clean generator output instead of polluted jtag-paths.json
 */

import * as fs from 'fs';
import * as path from 'path';
import type { PathMappingConfig } from '../generator/types/GeneratorTypes';

const cleanConfigPath = path.join(__dirname, '../.continuum/generator/path-mappings.json');
const packagePath = path.join(__dirname, '../package.json');

console.log('📦 Creating Node.js import map from clean generator output...');

// Check if clean generator output exists
if (!fs.existsSync(cleanConfigPath)) {
  console.log('⚠️ Clean generator output not found, skipping import map creation');
  console.log('   Run: npm run paths:clean to generate clean path mappings');
  process.exit(0);
}

// Load clean path mappings config
const config: PathMappingConfig = JSON.parse(fs.readFileSync(cleanConfigPath, 'utf8'));
const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));

// Convert only essential path mappings to Node.js import map format
const imports: Record<string, string> = {};

for (const [alias, pathMapping] of Object.entries(config.mappings)) {
  // Use the relativePath from our clean PathMapping structure
  const relativePath = pathMapping.relativePath;
  
  // For wildcard imports (like @shared/*)
  imports[`${alias}/*`] = `./dist/${relativePath}/*`;
  
  // For exact imports (like '../daemons/command-daemon/shared/CommandBase')
  if (alias === '../daemons/command-daemon/shared/CommandBase') {
    imports[alias] = `./dist/${relativePath}.js`;
  }
}

// Update package.json with imports field and generation metadata
packageJson.imports = imports;
packageJson._importsGenerated = {
  timestamp: new Date().toISOString(),
  source: '.continuum/generator/path-mappings.json',
  pathCount: Object.keys(imports).length,
  note: 'Generated from clean essential path mappings only'
};

// Write updated package.json
fs.writeFileSync(packagePath, JSON.stringify(packageJson, null, 2) + '\n');

console.log(`✅ Clean import map created with ${Object.keys(imports).length} essential aliases:`);
for (const [alias, target] of Object.entries(imports)) {
  console.log(`   ${alias} → ${target}`);
}