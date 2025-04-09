#!/usr/bin/env node
// Continuum CLI (Original filename kept for backward compatibility)

import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

// Get the directory of this file and real implementation
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const continuumPath = join(__dirname, 'continuum.js');

// Forward all arguments to continuum.js
const childProcess = spawn('node', [continuumPath, ...process.argv.slice(2)], {
  stdio: 'inherit'
});

// Handle process exit
childProcess.on('exit', (code) => {
  process.exit(code || 0);
});