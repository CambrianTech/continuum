#!/usr/bin/env node

/**
 * Simple utility to visualize an AI_CONFIG.md file
 * Usage: node visualize-config.js [path-to-config-file]
 */

const fs = require('fs');
const path = require('path');
const yaml = require('yaml');

// Get the file path from command line arguments
const configPath = process.argv[2] || 'AI_CONFIG.md';

// Read the file
try {
  // Check if file exists
  if (!fs.existsSync(configPath)) {
    console.error(`File not found: ${configPath}`);
    console.error(`Current working directory: ${process.cwd()}`);
    console.error('Available files:');
    fs.readdirSync('.').forEach(file => {
      console.error(`- ${file}`);
    });
    process.exit(1);
  }

  const content = fs.readFileSync(configPath, 'utf-8');
  console.log(`Successfully read file: ${configPath}`);
  
  // Extract YAML content
  const yamlMatch = content.match(/```yaml\n([\s\S]*?)```/);
  
  if (!yamlMatch) {
    console.error('No YAML configuration found in the file');
    console.error('File content preview:');
    console.error(content.substring(0, 200) + '...');
    process.exit(1);
  }
  
  const yamlContent = yamlMatch[1];
  console.log('Successfully extracted YAML content');
  
  try {
    const config = yaml.parse(yamlContent);
    console.log('Successfully parsed YAML content');
  
    // Display a visualization of the configuration
    console.log('\n=== AI Assistant Configuration Visualization ===\n');
  
    // Identity
  console.log(`🤖 ${config.identity.name || 'AI Assistant'}`);
  console.log(`   Role: ${config.identity.role}`);
  if (config.identity.purpose) {
    console.log(`   Purpose: ${config.identity.purpose}`);
  }
  
  // Behavior
  console.log('\n🧠 Behavior');
  if (config.behavior?.voice) {
    console.log(`   Voice: ${config.behavior.voice}`);
  }
  if (config.behavior?.autonomy) {
    console.log(`   Autonomy: ${config.behavior.autonomy}`);
  }
  if (config.behavior?.verbosity) {
    console.log(`   Verbosity: ${config.behavior.verbosity}`);
  }
  if (config.behavior?.risk_tolerance) {
    console.log(`   Risk Tolerance: ${config.behavior.risk_tolerance}`);
  }
  
  // Capabilities
  console.log('\n✅ Allowed Capabilities');
  if (config.capabilities?.allowed?.length) {
    config.capabilities.allowed.forEach(capability => {
      console.log(`   - ${capability}`);
    });
  } else {
    console.log('   None specified');
  }
  
  console.log('\n❌ Restricted Capabilities');
  if (config.capabilities?.restricted?.length) {
    config.capabilities.restricted.forEach(capability => {
      console.log(`   - ${capability}`);
    });
  } else {
    console.log('   None specified');
  }
  
  // Extensions
  if (config.extensions) {
    console.log('\n🧩 Extensions');
    Object.keys(config.extensions).forEach(ext => {
      console.log(`   ${ext}:`);
      const extension = config.extensions[ext];
      
      if (typeof extension === 'object') {
        Object.keys(extension).forEach(key => {
          const value = extension[key];
          if (Array.isArray(value)) {
            console.log(`     ${key}: ${value.join(', ')}`);
          } else {
            console.log(`     ${key}: ${value}`);
          }
        });
      }
    });
  }
  
  console.log('\n=== End of Configuration ===\n');
  
  } catch (error) {
    console.error(`Error parsing YAML: ${error.message}`);
    console.error(`YAML content: ${yamlContent}`);
    process.exit(1);
  }
} catch (error) {
  console.error(`Error: ${error.message}`);
  console.error(`Stack trace: ${error.stack}`);
  process.exit(1);
}