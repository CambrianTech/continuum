#!/usr/bin/env npx tsx
/**
 * Ensure Python genome training environment exists
 *
 * Auto-runs bootstrap.sh if micromamba environment is missing.
 * Called automatically during `npm start` to ensure Python always works.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

const REPO_ROOT = path.resolve(__dirname, '../../../..');
const GENOME_PYTHON_DIR = path.join(REPO_ROOT, '.continuum', 'genome', 'python');
const MICROMAMBA_ROOT = path.join(GENOME_PYTHON_DIR, 'micromamba');
const BOOTSTRAP_SCRIPT = path.join(GENOME_PYTHON_DIR, 'bootstrap.sh');
const ENV_NAME = 'jtag-genome-training';
const ENV_PATH = path.join(MICROMAMBA_ROOT, 'envs', ENV_NAME);

console.log('🔍 Checking Python genome training environment...');

// Check if environment exists
if (fs.existsSync(ENV_PATH)) {
  console.log('✅ Python environment found:', ENV_PATH);
  process.exit(0);
}

console.log('⚠️  Python environment not found');
console.log('📦 Creating environment (this will take 5-10 minutes on first run)...');
console.log('');

// Check if bootstrap script exists
if (!fs.existsSync(BOOTSTRAP_SCRIPT)) {
  console.error('❌ ERROR: bootstrap.sh not found at:', BOOTSTRAP_SCRIPT);
  console.error('');
  console.error('The genome training environment cannot be created without bootstrap.sh.');
  console.error('This file should exist at .continuum/genome/python/bootstrap.sh');
  console.error('');
  console.error('To restore from backup:');
  console.error('  cd src');
  console.error('  tar xzf backups/legacy-continuum-valuable-*.tgz -C /tmp');
  console.error('  cp /tmp/legacy-continuum-backup/genome-scripts/* ../../.continuum/genome/python/');
  process.exit(1);
}

// Run bootstrap script
try {
  console.log('🚀 Running bootstrap.sh...');
  console.log('');

  execSync('./bootstrap.sh', {
    cwd: GENOME_PYTHON_DIR,
    stdio: 'inherit',
    timeout: 600000 // 10 minutes max
  });

  console.log('');
  console.log('✅ Python environment created successfully!');
  console.log('');

} catch (error) {
  console.error('');
  console.error('❌ Failed to create Python environment');
  console.error('');
  console.error('Error:', (error as Error).message);
  console.error('');
  console.error('To manually bootstrap:');
  console.error('  cd .continuum/genome/python');
  console.error('  ./bootstrap.sh');
  console.error('');
  process.exit(1);
}
