/**
 * Sentinel Workspace Isolation Test
 *
 * Tests git-based workspace isolation for sentinel execution.
 */

import { SentinelWorkspace } from '../../system/sentinel/SentinelWorkspace';
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

async function testBranchIsolation(): Promise<void> {
  console.log('\n=== Test: Branch Isolation ===\n');

  // Use /tmp for test to avoid polluting the main repo
  const testDir = '/tmp/sentinel-workspace-test-' + Date.now();

  // Create a test git repo
  fs.mkdirSync(testDir, { recursive: true });
  execSync('git init', { cwd: testDir });
  execSync('git config user.email "test@test.com"', { cwd: testDir });
  execSync('git config user.name "Test"', { cwd: testDir });

  // Create initial file and commit
  fs.writeFileSync(path.join(testDir, 'file.txt'), 'original content');
  execSync('git add -A && git commit -m "initial"', { cwd: testDir });

  const originalBranch = execSync('git branch --show-current', { cwd: testDir, encoding: 'utf-8' }).trim();
  console.log(`Original branch: ${originalBranch}`);

  // Create workspace with branch isolation
  const handle = 'test-' + Date.now();
  const workspace = await SentinelWorkspace.create({
    callerDir: testDir,
    handle,
    isolation: 'branch',
    onFailure: 'delete',
  });

  console.log(`Workspace created:`);
  console.log(`  workingDir: ${workspace.workingDir}`);
  console.log(`  branch: ${workspace.workspace.branch}`);
  console.log(`  originalBranch: ${workspace.workspace.originalBranch}`);

  // Verify we're on the sentinel branch
  const currentBranch = execSync('git branch --show-current', { cwd: workspace.workingDir, encoding: 'utf-8' }).trim();
  console.log(`\nCurrent branch: ${currentBranch}`);

  if (currentBranch !== workspace.workspace.branch) {
    throw new Error(`Expected branch ${workspace.workspace.branch}, got ${currentBranch}`);
  }

  // Make some changes in the workspace
  fs.writeFileSync(path.join(workspace.workingDir, 'file.txt'), 'modified content');
  fs.writeFileSync(path.join(workspace.workingDir, 'new-file.txt'), 'new file');

  console.log('\nMade changes:');
  console.log('  - Modified file.txt');
  console.log('  - Created new-file.txt');

  // Verify changes exist
  const status = execSync('git status --porcelain', { cwd: workspace.workingDir, encoding: 'utf-8' });
  console.log(`\nGit status:\n${status}`);

  // Abort the workspace (should clean up)
  console.log('\nAborting workspace...');
  await workspace.abort('delete');

  // Verify we're back on original branch
  const finalBranch = execSync('git branch --show-current', { cwd: testDir, encoding: 'utf-8' }).trim();
  console.log(`\nFinal branch: ${finalBranch}`);

  if (finalBranch !== originalBranch) {
    throw new Error(`Expected to return to ${originalBranch}, but on ${finalBranch}`);
  }

  // Verify sentinel branch was deleted
  const branches = execSync('git branch', { cwd: testDir, encoding: 'utf-8' });
  if (branches.includes(workspace.workspace.branch)) {
    throw new Error(`Sentinel branch ${workspace.workspace.branch} should have been deleted`);
  }

  // Verify file.txt is back to original content
  const content = fs.readFileSync(path.join(testDir, 'file.txt'), 'utf-8');
  if (content !== 'original content') {
    throw new Error(`file.txt should be reverted, got: ${content}`);
  }

  // Verify new-file.txt doesn't exist
  if (fs.existsSync(path.join(testDir, 'new-file.txt'))) {
    throw new Error('new-file.txt should have been deleted on abort');
  }

  console.log('\n✅ Branch isolation test passed!');

  // Cleanup
  fs.rmSync(testDir, { recursive: true, force: true });
}

async function testSuccessfulCompletion(): Promise<void> {
  console.log('\n=== Test: Successful Completion ===\n');

  const testDir = '/tmp/sentinel-workspace-complete-' + Date.now();

  // Create a test git repo
  fs.mkdirSync(testDir, { recursive: true });
  execSync('git init', { cwd: testDir });
  execSync('git config user.email "test@test.com"', { cwd: testDir });
  execSync('git config user.name "Test"', { cwd: testDir });

  fs.writeFileSync(path.join(testDir, 'file.txt'), 'original');
  execSync('git add -A && git commit -m "initial"', { cwd: testDir });

  // Create workspace
  const handle = 'test-complete-' + Date.now();
  const workspace = await SentinelWorkspace.create({
    callerDir: testDir,
    handle,
    isolation: 'branch',
    onSuccess: 'merge',
  });

  console.log(`Workspace branch: ${workspace.workspace.branch}`);

  // Make changes and commit them
  fs.writeFileSync(path.join(workspace.workingDir, 'file.txt'), 'modified by sentinel');

  // Complete with merge
  console.log('Completing with merge...');
  const result = await workspace.complete('merge');

  console.log(`Result: merged=${result.merged}, branch=${result.branch}`);

  // Verify changes were merged
  const content = fs.readFileSync(path.join(testDir, 'file.txt'), 'utf-8');
  if (content !== 'modified by sentinel') {
    throw new Error(`Expected merged content, got: ${content}`);
  }

  console.log('\n✅ Successful completion test passed!');

  // Cleanup
  fs.rmSync(testDir, { recursive: true, force: true });
}

async function testNoIsolation(): Promise<void> {
  console.log('\n=== Test: No Isolation Mode ===\n');

  const testDir = '/tmp/sentinel-workspace-none-' + Date.now();

  // Create a test git repo
  fs.mkdirSync(testDir, { recursive: true });
  execSync('git init', { cwd: testDir });
  execSync('git config user.email "test@test.com"', { cwd: testDir });
  execSync('git config user.name "Test"', { cwd: testDir });

  fs.writeFileSync(path.join(testDir, 'file.txt'), 'original');
  execSync('git add -A && git commit -m "initial"', { cwd: testDir });

  // Create workspace with no isolation
  const handle = 'test-none-' + Date.now();
  const workspace = await SentinelWorkspace.create({
    callerDir: testDir,
    handle,
    isolation: 'none',
  });

  console.log(`Workspace workingDir: ${workspace.workingDir}`);
  console.log(`Expected callerDir: ${testDir}`);

  // Verify workingDir is the same as callerDir
  if (workspace.workingDir !== testDir) {
    throw new Error(`With isolation=none, workingDir should equal callerDir`);
  }

  console.log('\n✅ No isolation test passed!');

  // Cleanup
  fs.rmSync(testDir, { recursive: true, force: true });
}

async function main(): Promise<void> {
  console.log('Sentinel Workspace Isolation Tests');
  console.log('==================================\n');

  try {
    await testBranchIsolation();
    await testSuccessfulCompletion();
    await testNoIsolation();

    console.log('\n\n=============================');
    console.log('✅ All workspace tests passed!');
    console.log('=============================\n');
  } catch (err) {
    console.error('\n❌ Test failed:', err);
    process.exit(1);
  }
}

main();
