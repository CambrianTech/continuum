#!/usr/bin/env node
/**
 * Creates Node.js import map for path aliases
 * This solves the server-side runtime resolution issue
 */

const fs = require('fs');
const path = require('path');

const configPath = path.join(__dirname, '../jtag-paths.json');
const packagePath = path.join(__dirname, '../package.json');

console.log('📦 Creating Node.js import map for path aliases...');

// Load path mappings config
const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
const packageJson = JSON.parse(fs.readFileSync(packagePath, 'utf8'));

// Convert path mappings to Node.js import map format
const imports = {};

for (const [alias, relativePath] of Object.entries(config.pathMappings)) {
  // For wildcard imports (like @shared/*)
  imports[`${alias}/*`] = `./dist/${relativePath}/*`;
  
  // For exact imports (like '../daemons/command-daemon/shared/CommandBase')
  if (alias === '../daemons/command-daemon/shared/CommandBase') {
    imports[alias] = `./dist/${relativePath}.js`;
  }
}

// Update package.json with imports field
packageJson.imports = imports;

// Write updated package.json
fs.writeFileSync(packagePath, JSON.stringify(packageJson, null, 2) + '\n');

console.log('✅ Import map created:');
for (const [alias, target] of Object.entries(imports)) {
  console.log(`   ${alias} → ${target}`);
}