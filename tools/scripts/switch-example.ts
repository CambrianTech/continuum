#!/usr/bin/env tsx
/**
 * Simple Example Switcher - Cleanly switch between test-bench and widget-ui
 * 
 * Updates config/examples.json active_example field and updates the
 * ExampleConfig.ts to read from it consistently.
 */

import { readFileSync, writeFileSync } from 'fs';
import { resolve } from 'path';

interface ExampleConfig {
  active_example: string;
  examples: Record<string, any>;
}

const VALID_EXAMPLES = ['test-bench', 'widget-ui'] as const;
type ValidExample = typeof VALID_EXAMPLES[number];

function switchExample(targetExample: ValidExample): void {
  const configPath = resolve(__dirname, '../config/examples.json');
  
  try {
    console.log(`🔧 Switching to example: ${targetExample}`);
    
    // Read current config
    const configData = readFileSync(configPath, 'utf-8');
    const config: ExampleConfig = JSON.parse(configData);
    
    // Validate target example exists
    if (!config.examples[targetExample]) {
      throw new Error(`Example '${targetExample}' not found in config`);
    }
    
    // Already active?
    if (config.active_example === targetExample) {
      console.log(`✅ Example '${targetExample}' is already active`);
      return;
    }
    
    // Update active example
    config.active_example = targetExample;
    
    // Write back to file
    writeFileSync(configPath, JSON.stringify(config, null, 2) + '\n');
    
    console.log(`✅ Successfully switched to example: ${targetExample}`);
    console.log(`📍 Active example ports:`);
    console.log(`   HTTP: ${config.examples[targetExample].ports.http_server}`);
    console.log(`   WebSocket: ${config.examples[targetExample].ports.websocket_server}`);
    console.log(`📂 Directory: ${config.examples[targetExample].paths.directory}`);
    
  } catch (error) {
    console.error(`❌ Failed to switch example: ${error.message}`);
    process.exit(1);
  }
}

function showCurrentExample(): void {
  const configPath = resolve(__dirname, '../config/examples.json');
  
  try {
    const configData = readFileSync(configPath, 'utf-8');
    const config: ExampleConfig = JSON.parse(configData);
    
    console.log(`📍 Current active example: ${config.active_example}`);
    const activeConfig = config.examples[config.active_example];
    if (activeConfig) {
      console.log(`   Name: ${activeConfig.name}`);
      console.log(`   Description: ${activeConfig.description}`);
      console.log(`   HTTP Port: ${activeConfig.ports.http_server}`);
      console.log(`   WebSocket Port: ${activeConfig.ports.websocket_server}`);
      console.log(`   Directory: ${activeConfig.paths.directory}`);
    }
    
  } catch (error) {
    console.error(`❌ Failed to read current example: ${error.message}`);
    process.exit(1);
  }
}

// Command line interface
const args = process.argv.slice(2);

if (args.length === 0 || args[0] === '--current' || args[0] === '-c') {
  showCurrentExample();
  process.exit(0);
}

const targetExample = args[0];

if (!VALID_EXAMPLES.includes(targetExample as ValidExample)) {
  console.error(`❌ Invalid example: ${targetExample}`);
  console.error(`Valid examples: ${VALID_EXAMPLES.join(', ')}`);
  process.exit(1);
}

switchExample(targetExample as ValidExample);