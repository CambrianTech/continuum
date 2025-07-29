#!/usr/bin/env node
const { spawn } = require('child_process');
spawn('npx', ['tsx', __dirname + '/cli.ts', ...process.argv.slice(2)], { stdio: 'inherit' });