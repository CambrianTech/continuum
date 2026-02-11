/**
 * Sentinel Evidence Test
 *
 * Tests that BuildSentinel captures PROOF of its work.
 * The evidence should include actual build output, not just "trust me".
 */

import { BuildSentinel, type BuildResult } from '../../system/sentinel/BuildSentinel';
import { formatExecutionLog } from '../../system/sentinel/SentinelExecutionLog';
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

async function testBuildEvidenceCapture(): Promise<void> {
  console.log('\n=== Test: Build Evidence Capture ===\n');

  // Create a test directory with a failing TypeScript file
  const testDir = '/tmp/sentinel-evidence-test-' + Date.now();
  fs.mkdirSync(testDir, { recursive: true });

  // Write a simple tsconfig
  fs.writeFileSync(path.join(testDir, 'tsconfig.json'), JSON.stringify({
    compilerOptions: {
      target: 'ES2020',
      module: 'commonjs',
      strict: true,
      noEmit: true,
    },
    include: ['*.ts'],
  }, null, 2));

  // Write a TypeScript file with an error
  fs.writeFileSync(path.join(testDir, 'test.ts'), `
// This file has a type error
const x: number = "not a number";  // Error: string not assignable to number
console.log(x);
`);

  console.log('Test directory:', testDir);
  console.log('Created test.ts with intentional type error\n');

  // Run BuildSentinel
  const sentinel = new BuildSentinel({
    command: 'npx tsc --noEmit',
    workingDir: testDir,
    maxAttempts: 1,  // Just one attempt to capture the error
    canAutoFix: false,  // Don't try to fix
    useLLM: false,
    streamEvents: false,
  });

  console.log('Running BuildSentinel...\n');
  const result = await sentinel.run();

  console.log('Result:');
  console.log(`  success: ${result.success}`);
  console.log(`  escalated: ${result.escalated}`);
  console.log(`  attempts: ${result.attempts.length}`);

  // Check that we captured evidence
  const attempt = result.attempts[0];
  console.log('\nBuild Attempt Evidence:');
  console.log(`  rawOutput length: ${attempt.rawOutput.length} chars`);
  console.log(`  outputSummary:\n    ${attempt.outputSummary.split('\n').join('\n    ')}`);
  console.log(`  errors: ${attempt.errors.length}`);

  // Verify evidence was captured
  if (!attempt.rawOutput || attempt.rawOutput.length === 0) {
    throw new Error('Expected rawOutput to contain build output');
  }

  if (!attempt.rawOutput.includes('TS2322') && !attempt.rawOutput.includes('not assignable')) {
    throw new Error('Expected rawOutput to contain the type error message');
  }

  // Check execution log has evidence
  if (result.executionLog) {
    console.log('\n--- Execution Log with Evidence ---\n');
    console.log(formatExecutionLog(result.executionLog));

    // Verify evidence is in the actions
    const buildAction = result.executionLog.actions.find(a => a.type === 'build');
    if (!buildAction?.evidence) {
      throw new Error('Expected build action to have evidence');
    }

    if (!buildAction.evidence.output) {
      throw new Error('Expected evidence to have output');
    }

    if (!buildAction.evidence.verificationOutput) {
      throw new Error('Expected evidence to have verificationOutput');
    }

    console.log('Evidence verified:');
    console.log(`  output: ${buildAction.evidence.output.length} chars`);
    console.log(`  verificationOutput: ${buildAction.evidence.verificationOutput}`);
    console.log(`  verified: ${buildAction.evidence.verified}`);
  }

  console.log('\n✅ Build evidence capture test passed!');

  // Cleanup
  fs.rmSync(testDir, { recursive: true, force: true });
}

async function testSuccessEvidence(): Promise<void> {
  console.log('\n=== Test: Success Evidence ===\n');

  // Create a test directory with valid TypeScript
  const testDir = '/tmp/sentinel-success-test-' + Date.now();
  fs.mkdirSync(testDir, { recursive: true });

  fs.writeFileSync(path.join(testDir, 'tsconfig.json'), JSON.stringify({
    compilerOptions: {
      target: 'ES2020',
      module: 'commonjs',
      strict: true,
      noEmit: true,
    },
    include: ['*.ts'],
  }, null, 2));

  // Write valid TypeScript
  fs.writeFileSync(path.join(testDir, 'test.ts'), `
const x: number = 42;
console.log(x);
`);

  console.log('Test directory:', testDir);
  console.log('Created test.ts with valid code\n');

  const sentinel = new BuildSentinel({
    command: 'npx tsc --noEmit',
    workingDir: testDir,
    maxAttempts: 1,
    canAutoFix: false,
    useLLM: false,
  });

  console.log('Running BuildSentinel...\n');
  const result = await sentinel.run();

  console.log('Result:');
  console.log(`  success: ${result.success}`);

  // Verify success evidence
  const attempt = result.attempts[0];
  if (!attempt.success) {
    throw new Error('Expected build to succeed');
  }

  // Check execution log shows success proof
  if (result.executionLog) {
    const buildAction = result.executionLog.actions.find(a => a.type === 'build');

    if (!buildAction?.evidence?.verified) {
      throw new Error('Expected verified: true in evidence');
    }

    console.log('Success evidence:');
    console.log(`  verified: ${buildAction.evidence.verified}`);
    console.log(`  verificationOutput: ${buildAction.evidence.verificationOutput}`);
  }

  console.log('\n✅ Success evidence test passed!');

  // Cleanup
  fs.rmSync(testDir, { recursive: true, force: true });
}

async function main(): Promise<void> {
  console.log('Sentinel Evidence Tests');
  console.log('=======================\n');
  console.log('These tests verify that BuildSentinel captures PROOF of its work.\n');

  try {
    await testBuildEvidenceCapture();
    await testSuccessEvidence();

    console.log('\n\n================================');
    console.log('✅ All evidence tests passed!');
    console.log('================================\n');
  } catch (err) {
    console.error('\n❌ Test failed:', err);
    process.exit(1);
  }
}

main();
