/**
 * Test EntityGenerator - Generate GenomeAdapter entity from spec
 */

import * as fs from 'fs';
import * as path from 'path';
import { EntityGenerator } from './EntityGenerator';
import type { EntitySpec } from './EntityTypes';

async function testEntityGeneration(): Promise<void> {
  console.log('🧬 Testing EntityGenerator...\n');

  // Read spec
  const specPath = '/tmp/genome-adapter-entity-spec.json';
  const specContent = fs.readFileSync(specPath, 'utf-8');
  const spec: EntitySpec = JSON.parse(specContent);

  console.log(`📋 Spec: ${spec.name} (${spec.collectionName})`);
  console.log(`   Fields: ${Object.keys(spec.fields).length}`);
  console.log(`   Description: ${spec.description}\n`);

  // Create generator
  const rootPath = path.join(__dirname, '..');
  const generator = new EntityGenerator(rootPath);

  // Generate entity (output to /tmp for inspection)
  const outputDir = '/tmp/generated-entity-test';
  generator.generate(spec, outputDir);

  console.log(`\n✨ Entity generated successfully!`);
  console.log(`📂 Output: ${outputDir}/GenomeAdapterEntity.ts`);

  // Read and display generated entity
  const generatedPath = path.join(outputDir, 'GenomeAdapterEntity.ts');
  const generatedContent = fs.readFileSync(generatedPath, 'utf-8');

  console.log(`\n${'='.repeat(80)}`);
  console.log('GENERATED ENTITY:');
  console.log('='.repeat(80));
  console.log(generatedContent);
  console.log('='.repeat(80));
}

// Run test
testEntityGeneration().catch(error => {
  console.error('❌ Test failed:', error);
  process.exit(1);
});
