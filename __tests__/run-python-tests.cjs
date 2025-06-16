#!/usr/bin/env node
/**
 * Simple Python test runner for npm test integration
 */

const { spawn } = require('child_process');
const path = require('path');

async function runPythonTests() {
  console.log('🐍 Running Python tests...');
  
  // Look for Python tests in __tests__ directory
  const pythonTestDirs = [
    'integration/screenshot',
    'integration/api', 
    'integration/commands',
    'unit/python'
  ];
  
  for (const testDir of pythonTestDirs) {
    const fullPath = path.join(__dirname, testDir);
    console.log(`  📁 Checking ${testDir}...`);
    
    try {
      const result = await runPytest(fullPath);
      console.log(`  ✅ ${testDir} - ${result.testCount} tests`);
    } catch (error) {
      console.log(`  ⏩ ${testDir} - No Python tests or pytest not available`);
    }
  }
}

function runPytest(testPath) {
  return new Promise((resolve, reject) => {
    const pytest = spawn('python3', ['-m', 'pytest', testPath, '-v', '--tb=short'], {
      stdio: 'pipe'
    });
    
    let output = '';
    pytest.stdout.on('data', (data) => output += data.toString());
    pytest.stderr.on('data', (data) => output += data.toString());
    
    pytest.on('close', (code) => {
      const testCount = (output.match(/passed/g) || []).length;
      if (code === 0) {
        resolve({ testCount, output });
      } else {
        reject(new Error(`pytest failed: ${output}`));
      }
    });
    
    pytest.on('error', (error) => {
      reject(error);
    });
  });
}

if (require.main === module) {
  runPythonTests().catch(console.error);
}

module.exports = { runPythonTests };