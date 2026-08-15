#!/usr/bin/env npx tsx
/**
 * Test DaemonGenerator with cache-daemon spec
 */

import * as fs from 'fs';
import * as path from 'path';
import { DaemonGenerator } from './DaemonGenerator';
import type { DaemonSpec } from './DaemonTypes';

async function testDaemonGenerator(): Promise<void> {
  console.log('🧪 Testing DaemonGenerator');
  console.log('==========================\n');

  // Load spec
  const specPath = '/tmp/cache-daemon-spec.json';
  const specContent = fs.readFileSync(specPath, 'utf-8');
  const spec: DaemonSpec = JSON.parse(specContent);

  console.log(`📋 Loaded spec: ${spec.name}`);
  console.log(`   Description: ${spec.description}`);
  console.log(`   Jobs: ${spec.jobs.length}`);
  console.log(`   Events: ${spec.events?.length || 0}`);
  console.log(`   Lifecycle: ${spec.lifecycle ? 'yes' : 'no'}`);

  // Create generator
  const rootPath = path.join(__dirname, '..');
  const generator = new DaemonGenerator(rootPath);

  // Generate to /tmp to avoid touching real codebase
  const outputDir = '/tmp/generated-cache-daemon';

  // Clean up old output if it exists
  if (fs.existsSync(outputDir)) {
    console.log(`\n🧹 Cleaning old output: ${outputDir}`);
    fs.rmSync(outputDir, { recursive: true });
  }

  // Generate
  console.log(`\n🏗️  Generating daemon to: ${outputDir}`);
  generator.generate(spec, outputDir, { force: false });

  // Verify files were created
  const expectedFiles = [
    'shared/CacheDaemon.ts',
    'browser/CacheDaemonBrowser.ts',
    'server/CacheDaemonServer.ts'
  ];

  console.log(`\n✅ Verification:`);
  for (const file of expectedFiles) {
    const fullPath = path.join(outputDir, file);
    const exists = fs.existsSync(fullPath);
    const size = exists ? fs.statSync(fullPath).size : 0;
    console.log(`   ${exists ? '✅' : '❌'} ${file} (${size} bytes)`);
  }

  // Show a snippet of generated shared file
  const sharedPath = path.join(outputDir, 'shared/CacheDaemon.ts');
  const sharedContent = fs.readFileSync(sharedPath, 'utf-8');
  const lines = sharedContent.split('\n');
  
  console.log(`\n📄 Generated shared file (first 30 lines):`);
  console.log('─'.repeat(80));
  console.log(lines.slice(0, 30).join('\n'));
  console.log('─'.repeat(80));
  console.log(`... (${lines.length} total lines)`);

  console.log('\n🎉 DaemonGenerator test complete!');
}

testDaemonGenerator().catch(error => {
  console.error('❌ Test failed:', error);
  process.exit(1);
});
