#!/usr/bin/env node

/**
 * Simple schema validator that doesn't rely on ajv-cli
 * This avoids the security vulnerability in fast-json-patch
 */

import Ajv from 'ajv';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { glob } from 'glob';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.join(__dirname, '..');

// Load schema
try {
  // Initialize Ajv
  const ajv = new Ajv();

  // Load schema
  const schemaPath = path.join(rootDir, 'schema', 'continuum.schema.json');
  const schema = JSON.parse(fs.readFileSync(schemaPath, 'utf8'));
  
  // Compile schema
  const validate = ajv.compile(schema);
  
  // Find all template config files
  const configFiles = await glob('templates/*/config.json', { cwd: rootDir });
  
  if (configFiles.length === 0) {
    console.warn('No template config files found to validate');
    process.exit(0);
  }
  
  // Validate each template
  let hasErrors = false;
  
  for (const configFile of configFiles) {
    const configPath = path.join(rootDir, configFile);
    const configData = JSON.parse(fs.readFileSync(configPath, 'utf8'));
    
    const valid = validate(configData);
    if (!valid) {
      console.error(`❌ Validation failed for ${configFile}:`);
      console.error(validate.errors);
      hasErrors = true;
    } else {
      console.log(`✅ ${configFile} is valid`);
    }
  }
  
  if (hasErrors) {
    process.exit(1);
  }
  
  console.log('\n🎉 All templates validated successfully!');
} catch (error) {
  console.error(`Error validating schema: ${error.message}`);
  process.exit(1);
}
