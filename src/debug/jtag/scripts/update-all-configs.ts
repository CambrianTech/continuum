#!/usr/bin/env npx tsx

/**
 * Master Configuration Update Script
 * 
 * Single command to regenerate all configuration files from directory structure:
 * 1. Scans directory structure and generates unified-config.json
 * 2. Updates all config files (tsconfig.json, package.json, jtag-paths.json, import-map.json)
 */

import { UnifiedConfigGenerator } from './generate-unified-config';
import { ConfigFilesGenerator } from './generate-config-files';

async function main() {
  console.log('🚀 Master Configuration Update');
  console.log('=============================');
  console.log('Updating all configurations from directory structure...\n');

  try {
    // Step 1: Generate unified config from directory structure
    console.log('Step 1: Scanning directory structure...');
    const unifiedGenerator = new UnifiedConfigGenerator();
    await unifiedGenerator.generateConfig();

    console.log('\n' + '='.repeat(50) + '\n');

    // Step 2: Generate all config files from unified config
    console.log('Step 2: Generating all configuration files...');
    const configGenerator = new ConfigFilesGenerator();
    await configGenerator.generateAllConfigFiles();

    console.log('\n' + '🎉'.repeat(20));
    console.log('✅ All configurations updated successfully!');
    console.log('📁 Files updated:');
    console.log('   • unified-config.json (master source)');
    console.log('   • tsconfig.json (TypeScript paths)');
    console.log('   • package.json (import mappings)');
    console.log('   • jtag-paths.json (build system paths)');
    console.log('   • import-map.json (browser import map)');
    console.log('\n💡 Run this script whenever directory structure changes');

  } catch (error) {
    console.error('❌ Error updating configurations:', error);
    process.exit(1);
  }
}

if (require.main === module) {
  main();
}