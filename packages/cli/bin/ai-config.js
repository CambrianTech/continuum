#!/usr/bin/env node
// Simply redirect to the Continuum CLI

import { exec } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

// Display deprecation warning
console.log("⚠️  'ai-config' is deprecated. Please use 'continuum' instead.");
console.log("Learn more: https://github.com/CambrianTech/continuum");
console.log('');

// Get the directory of this file
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Full path to continuum.js
const continuumPath = join(__dirname, 'continuum.js');

// Forward all arguments to continuum.js
const args = process.argv.slice(2).join(' ');
const command = `node "${continuumPath}" ${args}`;

// Execute continuum.js
exec(command, { stdio: 'inherit' }, (error, stdout, stderr) => {
  if (stdout) process.stdout.write(stdout);
  if (stderr) process.stderr.write(stderr);
  if (error) process.exit(error.code || 1);
});