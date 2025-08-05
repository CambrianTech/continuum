#!/usr/bin/env npx tsx

/**
 * Smart deployment - usually just installs, handles edge cases gracefully
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

function getVersionInfo() {
  const packageJson = JSON.parse(fs.readFileSync('package.json', 'utf8'));
  const version = packageJson.version;
  const tarballName = `continuum-jtag-${version}.tgz`;
  return { version, tarballName };
}

function checkTestBenchDependency(): boolean {
  const testBenchPath = 'examples/test-bench/package.json';
  if (!fs.existsSync(testBenchPath)) return false;
  
  const testBench = JSON.parse(fs.readFileSync(testBenchPath, 'utf8'));
  const currentDep = testBench.dependencies?.['@continuum/jtag'];
  const { tarballName } = getVersionInfo();
  const expectedDep = `file:../../${tarballName}`;
  
  return currentDep === expectedDep;
}

function smartDeploy(): void {
  console.log('🧠 Smart deployment analysis...\n');
  
  const { version, tarballName } = getVersionInfo();
  const tarballExists = fs.existsSync(tarballName);
  const testBenchCorrect = checkTestBenchDependency();
  
  console.log(`📦 Version: ${version}`);
  console.log(`📋 Tarball exists: ${tarballExists ? '✅' : '❌'}`);
  console.log(`📋 Test-bench dependency correct: ${testBenchCorrect ? '✅' : '❌'}`);
  
  if (!tarballExists) {
    console.log('\n❌ Tarball missing - this should not happen in smart-build');
    process.exit(1);
  }
  
  if (testBenchCorrect) {
    console.log('\n✅ Test-bench already configured correctly - skipping deployment');
    return;
  }
  
  console.log('\n🔧 Updating test-bench dependency...');
  
  // Quick dependency update only
  const testBenchPath = 'examples/test-bench/package.json';
  const testBench = JSON.parse(fs.readFileSync(testBenchPath, 'utf8'));
  
  if (!testBench.dependencies) testBench.dependencies = {};
  testBench.dependencies['@continuum/jtag'] = `file:../../${tarballName}`;
  
  fs.writeFileSync(testBenchPath, JSON.stringify(testBench, null, 2) + '\n');
  console.log(`✅ Updated test-bench to use ${tarballName}`);
  
  // Only clean install if really needed
  const nodeModulesPath = 'examples/test-bench/node_modules/@continuum/jtag';
  if (fs.existsSync(nodeModulesPath)) {
    console.log('🗑️  Cleaning old JTAG installation...');
    fs.rmSync(nodeModulesPath, { recursive: true, force: true });
  }
  
  console.log('📥 Installing updated dependency...');
  try {
    execSync('npm install', { 
      cwd: 'examples/test-bench',
      stdio: 'pipe' // Quiet install
    });
    console.log('✅ Smart deployment complete!');
  } catch (error) {
    console.log('⚠️  Quick install failed, trying full clean install...');
    
    // Fallback to full clean only if needed
    const cleanPaths = [
      'examples/test-bench/node_modules',
      'examples/test-bench/package-lock.json'
    ];
    
    for (const cleanPath of cleanPaths) {
      if (fs.existsSync(cleanPath)) {
        fs.rmSync(cleanPath, { recursive: true, force: true });
        console.log(`🗑️  Removed ${path.basename(cleanPath)}`);
      }
    }
    
    execSync('npm install', { 
      cwd: 'examples/test-bench',
      stdio: 'inherit'
    });
    console.log('✅ Full clean deployment complete!');
  }
}

if (require.main === module) {
  smartDeploy();
}

export { smartDeploy };