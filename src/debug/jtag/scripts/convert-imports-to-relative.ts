#!/usr/bin/env npx tsx

/**
 * Converts path alias imports to relative imports in compiled JavaScript files
 * This fixes Node.js runtime resolution issues using modular utilities
 */

import * as fs from 'fs';
import * as path from 'path';
import type { PathMappingConfig } from '../generator/types/GeneratorTypes';
import { ImportPathResolver } from './utils/ImportPathResolver';
import { FileDiscovery, BatchProcessor } from './utils/FileProcessor';

const cleanConfigPath = path.join(__dirname, '../.continuum/generator/path-mappings.json');
const distPath = path.join(__dirname, '../dist');

console.log('🔄 Converting path alias imports to relative imports in JavaScript files...');

// Check if clean generator output exists
if (!fs.existsSync(cleanConfigPath)) {
  console.log('⚠️ Clean generator output not found, skipping import conversion');
  console.log('   Run: npm run paths:clean to generate clean path mappings');
  process.exit(0);
}

// Load clean path mappings config
const config: PathMappingConfig = JSON.parse(fs.readFileSync(cleanConfigPath, 'utf8'));
const aliasMappings: Record<string, string> = {};

// Convert clean PathMapping format to simple alias mapping
for (const [alias, pathMapping] of Object.entries(config.mappings)) {
  aliasMappings[alias] = pathMapping.relativePath;
}

// Initialize modular components
const pathResolver = new ImportPathResolver(aliasMappings, distPath);
const batchProcessor = new BatchProcessor();

// Discover all JavaScript files
const jsFiles = FileDiscovery.findJavaScriptFiles(distPath);
console.log(`📁 Found ${jsFiles.length} JavaScript files to process`);

// Process all files using the batch processor
const stats = batchProcessor.processFiles(
  jsFiles,
  distPath,
  // Transformation function
  (content: string, filePath: string) => {
    const result = pathResolver.resolveImportsInContent(content, filePath);
    return {
      content: result.content,
      metadata: result.replacements
    };
  },
  // Callback for each processed file
  (result) => {
    const { filePath, modified, metadata: replacements } = result;
    
    console.log(`\n🔍 Processing: ${filePath}`);
    
    if (modified && replacements.length > 0) {
      replacements.forEach(({ from, to }) => {
        console.log(`   🔄 ${from} -> ${to}`);
      });
      console.log(`✅ Updated: ${filePath} (${replacements.length} replacements)`);
    } else {
      console.log(`⏭️  No changes: ${filePath}`);
    }
    
    // Update total replacement count
    batchProcessor.getStats().totalReplacements += replacements.length;
  }
);

// Display final statistics
console.log(`\n🎉 Conversion complete!`);
console.log(`   Files processed: ${stats.filesProcessed}`);
console.log(`   Files modified: ${stats.filesModified}`);
console.log(`   Total replacements: ${stats.totalReplacements}`);