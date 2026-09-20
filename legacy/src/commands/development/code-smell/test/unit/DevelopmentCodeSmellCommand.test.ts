#!/usr/bin/env tsx
/**
 * DevelopmentCodeSmell Command Unit Tests
 *
 * TDD pattern: creates temp files with known smells, runs the scanner,
 * verifies each smell is detected. Then removes the smell and verifies
 * the scanner reports clean.
 *
 * Run: npx tsx commands/development/code-smell/test/unit/DevelopmentCodeSmellCommand.test.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { DevelopmentCodeSmellServerCommand } from '../../server/DevelopmentCodeSmellServerCommand';
import type { DevelopmentCodeSmellParams } from '../../shared/DevelopmentCodeSmellTypes';
import { generateUUID } from '@system/core/types/CrossPlatformUUID';

console.log('🧪 DevelopmentCodeSmell Command Unit Tests\n');

let passed = 0;
let failed = 0;

function assert(condition: boolean, message: string): void {
  if (!condition) {
    failed++;
    console.log(`  ❌ ${message}`);
  } else {
    passed++;
    console.log(`  ✅ ${message}`);
  }
}

// Create a temp directory with known smells
const tmpDir = path.join(os.tmpdir(), `code-smell-test-${Date.now()}`);

function setup() {
  fs.mkdirSync(tmpDir, { recursive: true });
}

function teardown() {
  fs.rmSync(tmpDir, { recursive: true, force: true });
}

function writeFile(name: string, content: string) {
  const filePath = path.join(tmpDir, name);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, 'utf-8');
}

function makeParams(overrides: Partial<DevelopmentCodeSmellParams> = {}): DevelopmentCodeSmellParams {
  return {
    context: 'server',
    sessionId: generateUUID(),
    userId: generateUUID(),
    ...overrides,
  } as DevelopmentCodeSmellParams;
}

// Instantiate server command (doesn't need real daemon for unit tests)
const command = new DevelopmentCodeSmellServerCommand('server' as any, '', null as any);

// ── Test Suite ─────────────────────────────────────────────────

async function testAnyCastDetection() {
  console.log('1. Any Cast Detection');

  writeFile('smelly.ts', `
    const x = foo as any;
    const y: any = bar;
    const z = baz as string; // not a smell
  `);

  const result = await command.execute(makeParams({
    category: 'any-casts',
    path: path.relative(path.resolve(__dirname, '../../../..'), tmpDir),
  }));

  assert(result.success, 'Scan completed');
  const anyCasts = result.categories.find(c => c.category === 'any-casts');
  assert(anyCasts !== undefined, 'any-casts category returned');
  assert(anyCasts!.count >= 2, `Detected ${anyCasts!.count} any casts (expected >= 2)`);
}

async function testRawExecuteDetection() {
  console.log('\n2. Raw Commands.execute Detection');

  writeFile('raw-execute.ts', `
    import { Commands } from '../../system/core/shared/Commands';
    const result = await Commands.execute('ping', {});
    const typed = await Ping.execute({}); // not a smell
  `);

  const result = await command.execute(makeParams({
    category: 'raw-execute',
    path: path.relative(path.resolve(__dirname, '../../../..'), tmpDir),
  }));

  assert(result.success, 'Scan completed');
  const raw = result.categories.find(c => c.category === 'raw-execute');
  assert(raw !== undefined, 'raw-execute category returned');
  assert(raw!.count >= 1, `Detected ${raw!.count} raw execute calls (expected >= 1)`);
}

async function testGodClassDetection() {
  console.log('\n3. God Class Detection');

  // Write a 600-line file
  const lines = Array.from({ length: 600 }, (_, i) => `// line ${i + 1}`).join('\n');
  writeFile('god-class.ts', lines);

  // Write a normal 100-line file
  const smallLines = Array.from({ length: 100 }, (_, i) => `// line ${i + 1}`).join('\n');
  writeFile('small-file.ts', smallLines);

  const result = await command.execute(makeParams({
    category: 'god-class',
    path: path.relative(path.resolve(__dirname, '../../../..'), tmpDir),
  }));

  assert(result.success, 'Scan completed');
  const gods = result.categories.find(c => c.category === 'god-class');
  assert(gods !== undefined, 'god-class category returned');
  assert(gods!.count >= 1, `Detected ${gods!.count} god classes (expected >= 1)`);

  // Verify the small file is NOT flagged
  const flaggedFiles = gods!.locations.map(l => l.file);
  const hasSmallFile = flaggedFiles.some(f => f.includes('small-file.ts'));
  assert(!hasSmallFile, 'Small file not flagged as god class');
}

async function testCleanScan() {
  console.log('\n4. Clean Code Detection');

  // Write a clean file
  const cleanDir = path.join(os.tmpdir(), `code-smell-clean-${Date.now()}`);
  fs.mkdirSync(cleanDir, { recursive: true });
  const cleanFile = path.join(cleanDir, 'clean.ts');
  fs.writeFileSync(cleanFile, `
    import { Ping } from '@commands/ping/shared/PingTypes';
    const result = await Ping.execute({});
    const typed: string = result.timestamp;
  `, 'utf-8');

  const result = await command.execute(makeParams({
    path: path.relative(path.resolve(__dirname, '../../../..'), cleanDir),
  }));

  assert(result.success, 'Scan completed');
  const anyCasts = result.categories.find(c => c.category === 'any-casts');
  const rawExec = result.categories.find(c => c.category === 'raw-execute');
  const godClass = result.categories.find(c => c.category === 'god-class');
  assert(anyCasts?.count === 0, 'No any casts in clean file');
  assert(rawExec?.count === 0, 'No raw execute in clean file');
  assert(godClass?.count === 0, 'No god classes in clean file');

  fs.rmSync(cleanDir, { recursive: true, force: true });
}

async function testSummaryFormat() {
  console.log('\n5. Summary Format');

  const result = await command.execute(makeParams({
    path: path.relative(path.resolve(__dirname, '../../../..'), tmpDir),
  }));

  assert(result.summary.includes('📊 Total:'), 'Summary includes total line');
  assert(typeof result.totalSmells === 'number', 'totalSmells is a number');
  assert(result.totalSmells >= 0, 'totalSmells is non-negative');
}

async function testVerboseMode() {
  console.log('\n6. Verbose Mode');

  const normal = await command.execute(makeParams({
    category: 'god-class',
    verbose: false,
  }));

  const verbose = await command.execute(makeParams({
    category: 'god-class',
    verbose: true,
  }));

  assert(normal.success && verbose.success, 'Both modes succeed');
  const normalGods = normal.categories.find(c => c.category === 'god-class');
  const verboseGods = verbose.categories.find(c => c.category === 'god-class');
  // Verbose should show all locations, normal caps at 15
  assert(verboseGods!.locations.length >= normalGods!.locations.length, 'Verbose shows >= normal locations');
}

// ── Run ────────────────────────────────────────────────────────

async function run() {
  setup();
  try {
    await testAnyCastDetection();
    await testRawExecuteDetection();
    await testGodClassDetection();
    await testCleanScan();
    await testSummaryFormat();
    await testVerboseMode();
  } finally {
    teardown();
  }

  console.log(`\n${'═'.repeat(40)}`);
  console.log(`  ${passed} passed, ${failed} failed`);
  console.log(`${'═'.repeat(40)}\n`);

  if (failed > 0) process.exit(1);
}

run().catch(err => {
  console.error('Test runner error:', err);
  teardown();
  process.exit(1);
});
