#!/usr/bin/env node
/**
 * Simple unified test runner - tests the strategy immediately
 */

const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');

async function runTests() {
  console.log('🧪 Testing our test strategy...\n');
  
  // 1. Test JavaScript tests first (these should work)
  console.log('1️⃣ Testing JavaScript unit tests...');
  try {
    await runCommand('npx', ['jest', '__tests__/unit/js/commands/ScreenshotCommand.test.cjs', '--config', 'jest.config.cjs', '--no-coverage']);
    console.log('✅ JS tests work\n');
  } catch (error) {
    console.log('❌ JS tests failed:', error.message, '\n');
  }
  
  // 2. Check for Python venv 
  console.log('2️⃣ Checking Python venv setup...');
  const venvPaths = [
    '.continuum/venv',
    'python-client/venv', 
    '.venv'
  ];
  
  let pythonPath = 'python3';
  for (const venvPath of venvPaths) {
    const venvPython = path.join(venvPath, 'bin', 'python');
    if (fs.existsSync(venvPython)) {
      pythonPath = venvPython;
      console.log(`✅ Found venv at ${venvPath}`);
      break;
    }
  }
  
  if (pythonPath === 'python3') {
    console.log('⚠️  No venv found, using system python3\n');
  }
  
  // 3. Test if we can import continuum_client
  console.log('3️⃣ Testing Python import...');
  try {
    const result = await runCommand(pythonPath, ['-c', 'import sys; print("Python:", sys.version); import continuum_client; print("✅ continuum_client imported")']);
    console.log(result);
  } catch (error) {
    console.log('❌ Python import failed - we need to handle this case\n');
  }
  
  // 4. Check what Python tests we actually have
  console.log('4️⃣ Scanning for redundant tests...');
  const pythonTests = [];
  const jsTests = [];
  
  // Quick scan
  function scanDir(dir, tests, ext) {
    if (!fs.existsSync(dir)) return;
    const files = fs.readdirSync(dir, { withFileTypes: true });
    for (const file of files) {
      if (file.isFile() && file.name.includes('test') && file.name.endsWith(ext)) {
        tests.push(path.join(dir, file.name));
      } else if (file.isDirectory()) {
        scanDir(path.join(dir, file.name), tests, ext);
      }
    }
  }
  
  scanDir('python-client', pythonTests, '.py');
  scanDir('__tests__', jsTests, '.cjs');
  
  console.log(`Found ${pythonTests.length} Python tests, ${jsTests.length} JS tests`);
  
  // 5. Simple redundancy check
  const redundant = [];
  for (const pyTest of pythonTests) {
    const testName = path.basename(pyTest, '.py').replace('test_', '');
    const hasJSEquivalent = jsTests.some(jsTest => 
      jsTest.toLowerCase().includes(testName.toLowerCase())
    );
    if (hasJSEquivalent) {
      redundant.push({ python: pyTest, testName });
    }
  }
  
  console.log(`\n📊 Analysis:`);
  console.log(`  - ${redundant.length} potentially redundant Python tests`);
  console.log(`  - Focus on core JS tests first`);
  console.log(`  - Keep Python tests that are Python-specific only`);
  
  if (redundant.length > 0) {
    console.log(`\n🔍 Redundant tests to review:`);
    redundant.forEach(r => console.log(`  - ${r.testName}: ${r.python}`));
  }
}

function runCommand(cmd, args) {
  return new Promise((resolve, reject) => {
    const proc = spawn(cmd, args, { stdio: 'pipe' });
    let output = '';
    
    proc.stdout.on('data', (data) => output += data.toString());
    proc.stderr.on('data', (data) => output += data.toString());
    
    proc.on('close', (code) => {
      if (code === 0) {
        resolve(output);
      } else {
        reject(new Error(`${cmd} failed: ${output}`));
      }
    });
    
    proc.on('error', (error) => {
      reject(error);
    });
  });
}

if (require.main === module) {
  runTests().catch(console.error);
}

module.exports = { runTests };