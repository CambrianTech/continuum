#!/usr/bin/env node
import { openSync } from 'fs';
import { spawn } from 'child_process';

const args = process.argv.slice(2);
let cwd = process.cwd();
let logPath = null;
let ulimitVirtualMemoryKb = null;
const env = { ...process.env };
let i = 0;

for (; i < args.length; i += 1) {
  const arg = args[i];
  if (arg === '--') {
    i += 1;
    break;
  }
  if (arg === '--cwd') {
    cwd = args[++i];
    continue;
  }
  if (arg === '--log') {
    logPath = args[++i];
    continue;
  }
  if (arg === '--env') {
    const assignment = args[++i];
    const equalsIndex = assignment.indexOf('=');
    if (equalsIndex <= 0) {
      throw new Error(`Invalid --env assignment: ${assignment}`);
    }
    env[assignment.slice(0, equalsIndex)] = assignment.slice(equalsIndex + 1);
    continue;
  }
  if (arg === '--ulimit-v-kb') {
    ulimitVirtualMemoryKb = args[++i];
    continue;
  }
  throw new Error(`Unknown option: ${arg}`);
}

let command = args[i];
let commandArgs = args.slice(i + 1);
if (!command) {
  throw new Error('Usage: spawn-detached.mjs [--cwd DIR] [--log FILE] [--env K=V] -- command [args...]');
}

if (ulimitVirtualMemoryKb) {
  commandArgs = [
    '-lc',
    'ulimit -v "$1" 2>/dev/null || true; shift; exec "$@"',
    'spawn-detached-ulimit',
    String(ulimitVirtualMemoryKb),
    command,
    ...commandArgs,
  ];
  command = '/bin/bash';
}

const out = logPath ? openSync(logPath, 'a') : 'ignore';
const err = logPath ? out : 'ignore';
const child = spawn(command, commandArgs, {
  cwd,
  env,
  detached: true,
  stdio: ['ignore', out, err],
});

child.unref();
console.log(child.pid);
