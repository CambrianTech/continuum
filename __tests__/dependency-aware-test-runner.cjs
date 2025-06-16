#!/usr/bin/env node
/**
 * Dependency-Aware Test Runner
 * Runs command tests in dependency order
 */

const { spawn } = require('child_process');
const { testDependencyOrder } = require('./scan-command-dependencies.cjs');
const fs = require('fs');
const path = require('path');

async function runCommandTests() {
  console.log('🎯 Running dependency-aware command tests...\n');
  
  // Get dependency-sorted test order
  const testOrder = testDependencyOrder();
  console.log('\n▶️  Starting tests in dependency order...\n');
  
  const results = [];
  
  for (const moduleName of testOrder) {
    // Try storage directory first, then widgets, then commands
    let moduleDir = path.join('src/storage', moduleName);
    let moduleType = '🗄️';
    
    if (!fs.existsSync(moduleDir)) {
      moduleDir = path.join('src/ui/components', moduleName);
      moduleType = '🎨';
    }
    
    if (!fs.existsSync(moduleDir)) {
      moduleDir = path.join('src/commands/core', moduleName);
      moduleType = '📦';
    }
    
    const packagePath = path.join(moduleDir, 'package.json');
    
    if (!fs.existsSync(packagePath)) {
      console.log(`⏩ Skipping ${moduleName} (no package.json)`);
      continue;
    }
    
    try {
      const packageData = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
      if (!packageData.scripts?.test) {
        console.log(`⏩ Skipping ${moduleName} (no test script)`);
        continue;
      }
      
      console.log(`🧪 Testing ${moduleType} ${moduleName}...`);
      const result = await runTest(moduleDir);
      results.push({ command: moduleName, ...result });
      
      if (result.success) {
        console.log(`✅ ${moduleName} tests passed\n`);
      } else {
        console.log(`❌ ${moduleName} tests failed\n`);
        console.log(`Error: ${result.error}\n`);
      }
      
    } catch (error) {
      console.log(`❌ ${moduleName} error: ${error.message}\n`);
      results.push({ command: moduleName, success: false, error: error.message });
    }
  }
  
  // Summary
  console.log('📊 Test Summary:');
  const passed = results.filter(r => r.success).length;
  const failed = results.filter(r => !r.success).length;
  
  console.log(`✅ Passed: ${passed}`);
  console.log(`❌ Failed: ${failed}`);
  console.log(`📦 Total: ${results.length}`);
  
  if (failed > 0) {
    console.log('\n💥 Failed commands:');
    results.filter(r => !r.success).forEach(r => {
      console.log(`  - ${r.command}: ${r.error}`);
    });
  }
}

function runTest(commandDir) {
  return new Promise((resolve) => {
    const proc = spawn('npm', ['test'], { 
      cwd: commandDir,
      stdio: 'pipe'
    });
    
    let output = '';
    proc.stdout.on('data', (data) => output += data.toString());
    proc.stderr.on('data', (data) => output += data.toString());
    
    proc.on('close', (code) => {
      resolve({
        success: code === 0,
        error: code !== 0 ? output : null,
        output: output
      });
    });
    
    proc.on('error', (error) => {
      resolve({
        success: false,
        error: error.message,
        output: output
      });
    });
  });
}

if (require.main === module) {
  runCommandTests().catch(console.error);
}

module.exports = { runCommandTests };