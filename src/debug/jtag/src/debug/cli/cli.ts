#!/usr/bin/env tsx
/**
 * JTAG CLI - Forward to npm scripts
 */

import { execSync } from 'child_process';

const [command, ...args] = process.argv.slice(2);

if (!command) {
  console.log('Usage: ./jtag <command> [...args]');
  process.exit(0);
}

// Forward to the npm script with parameters
const argsString = args.join(' ');
try {
  execSync(`npm run ${command} -- ${argsString}`, { 
    stdio: 'inherit',
    cwd: process.cwd()
  });
} catch (error) {
  process.exit(1);
}