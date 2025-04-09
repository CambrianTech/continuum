#!/usr/bin/env node

/**
 * Simple utility to check if an AI_CONFIG.md file exists
 * This is a simplified version that doesn't depend on the yaml module
 * Usage: node visualize-config-simple.js [path-to-config-file]
 */

const fs = require('fs');
const path = require('path');

// Get the file path from command line arguments
const configPath = process.argv[2] || 'AI_CONFIG.md';

console.log(`Checking file: ${configPath}`);

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
  console.log(`File size: ${content.length} bytes`);
  
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
  console.log(`YAML content size: ${yamlContent.length} bytes`);
  
  // Simple validation - check for required fields
  if (yamlContent.includes('ai_protocol_version') && 
      yamlContent.includes('identity') && 
      yamlContent.includes('role')) {
    console.log('Basic validation passed - required fields found');
    console.log('Example script completed successfully');
  } else {
    console.error('Basic validation failed - missing required fields');
    process.exit(1);
  }
  
} catch (error) {
  console.error(`Error: ${error.message}`);
  console.error(`Stack trace: ${error.stack}`);
  process.exit(1);
}