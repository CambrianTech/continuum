/**
 * migrate-sandbox-to-git.ts - One-time migration of persona sandbox workspaces to git repos
 *
 * Scans .continuum/personas/{id}/workspace/ for non-empty directories without .git,
 * initializes git repos, and creates initial commits preserving existing work.
 *
 * Safe to re-run: skips workspaces that already have .git initialized.
 *
 * Usage: npx tsx scripts/migrate-sandbox-to-git.ts
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

const JTAG_ROOT = path.resolve(__dirname, '..');
const PERSONAS_DIR = path.join(JTAG_ROOT, '.continuum', 'personas');

interface MigrationResult {
  readonly personaId: string;
  readonly workspacePath: string;
  readonly fileCount: number;
  readonly status: 'migrated' | 'skipped-empty' | 'skipped-already-git' | 'failed';
  readonly error?: string;
}

function countFiles(dir: string): number {
  let count = 0;
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.name === '.git' || entry.name === 'node_modules') continue;
    const fullPath = path.join(dir, entry.name);
    if (entry.isFile()) {
      count++;
    } else if (entry.isDirectory()) {
      count += countFiles(fullPath);
    }
  }
  return count;
}

function migrateWorkspace(personaId: string, workspacePath: string): MigrationResult {
  // Skip if already a git repo
  if (fs.existsSync(path.join(workspacePath, '.git'))) {
    return { personaId, workspacePath, fileCount: 0, status: 'skipped-already-git' };
  }

  // Count files (skip node_modules)
  const fileCount = countFiles(workspacePath);
  if (fileCount === 0) {
    return { personaId, workspacePath, fileCount: 0, status: 'skipped-empty' };
  }

  try {
    const opts = { cwd: workspacePath, stdio: 'pipe' as const };

    // Initialize git repo
    execSync('git init', opts);

    // Set identity — use persona ID as placeholder; proper names set when project workspaces are created
    execSync(`git config user.name "AI Persona (${personaId.slice(0, 8)})"`, opts);
    execSync(`git config user.email "${personaId}@continuum.local"`, opts);

    // Create .gitignore for common build artifacts
    const gitignore = 'node_modules/\ndist/\n.DS_Store\n*.log\n';
    fs.writeFileSync(path.join(workspacePath, '.gitignore'), gitignore);

    // Stage all files
    execSync('git add .', opts);

    // Initial commit
    execSync('git commit -m "Initial commit - migrated from sandbox workspace"', opts);

    console.log(`  Migrated: ${personaId.slice(0, 8)}... (${fileCount} files)`);
    return { personaId, workspacePath, fileCount, status: 'migrated' };

  } catch (error: any) {
    console.error(`  Failed: ${personaId.slice(0, 8)}... - ${error.message}`);
    return { personaId, workspacePath, fileCount, status: 'failed', error: error.message };
  }
}

function main(): void {
  console.log('Migrating persona sandbox workspaces to git repos...\n');

  if (!fs.existsSync(PERSONAS_DIR)) {
    console.log('No personas directory found. Nothing to migrate.');
    return;
  }

  const personaDirs = fs.readdirSync(PERSONAS_DIR, { withFileTypes: true })
    .filter(d => d.isDirectory() && d.name !== '.DS_Store');

  const results: MigrationResult[] = [];

  for (const dir of personaDirs) {
    const workspacePath = path.join(PERSONAS_DIR, dir.name, 'workspace');
    if (!fs.existsSync(workspacePath) || !fs.statSync(workspacePath).isDirectory()) {
      continue;
    }

    const result = migrateWorkspace(dir.name, workspacePath);
    results.push(result);
  }

  // Summary
  const migrated = results.filter(r => r.status === 'migrated');
  const skippedGit = results.filter(r => r.status === 'skipped-already-git');
  const skippedEmpty = results.filter(r => r.status === 'skipped-empty');
  const failed = results.filter(r => r.status === 'failed');

  console.log('\n--- Migration Summary ---');
  console.log(`Migrated:      ${migrated.length} workspaces (${migrated.reduce((s, r) => s + r.fileCount, 0)} total files)`);
  console.log(`Already git:   ${skippedGit.length} workspaces`);
  console.log(`Empty:         ${skippedEmpty.length} workspaces`);
  if (failed.length > 0) {
    console.log(`Failed:        ${failed.length} workspaces`);
    for (const f of failed) {
      console.log(`  - ${f.personaId}: ${f.error}`);
    }
  }
}

main();
