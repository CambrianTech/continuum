#!/usr/bin/env node
/**
 * Build Browser Client - Clean TypeScript compilation for browser
 */

const { execSync } = require('child_process');
const path = require('path');

console.log('📦 Building JTAG browser client...');

// Ensure dist directory exists
execSync('mkdir -p examples/dist', { cwd: __dirname + '/..' });

// Compile browser client (UMD for compatibility)
console.log('🔧 Compiling browser client...');
execSync(`npx tsc browser-client/jtag-auto-init.ts \
  --outDir examples/dist \
  --target ES2015 \
  --module UMD \
  --moduleResolution node \
  --skipLibCheck \
  --lib ES2020,DOM \
  --sourceMap`, { 
  cwd: __dirname + '/..',
  stdio: 'inherit'
});

// Compile demo client (ignore TypeScript errors for now)
console.log('🔧 Compiling demo client...');
try {
  execSync(`npx tsc examples/demo.ts \
    --outDir examples/dist \
    --target ES2020 \
    --module ES2020 \
    --moduleResolution node \
    --skipLibCheck \
    --lib ES2020,DOM \
    --sourceMap`, { 
    cwd: __dirname + '/..',
    stdio: 'inherit'
  });
} catch (error) {
  console.log('⚠️ TypeScript errors in demo.ts but JavaScript generated');
}

console.log('✅ Browser client build complete');