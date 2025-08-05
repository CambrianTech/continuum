#!/usr/bin/env npx tsx

/**
 * Smart deployment - tries quick install first, falls back to full clean only when needed
 */

import * as fs from 'fs';
import { execSync } from 'child_process';

function getVersionInfo() {
  const packageJson = JSON.parse(fs.readFileSync('package.json', 'utf8'));
  const version = packageJson.version;
  const tarballName = `continuum-jtag-${version}.tgz`;
  return { version, tarballName };
}

function updateTestBenchDependency(tarballName: string): void {
  const testBenchPath = 'examples/test-bench/package.json';
  if (!fs.existsSync(testBenchPath)) {
    console.log('❌ Test-bench not found');
    process.exit(1);
  }
  
  const testBench = JSON.parse(fs.readFileSync(testBenchPath, 'utf8'));
  const currentDep = testBench.dependencies?.['@continuum/jtag'];
  const expectedDep = `file:../../${tarballName}`;
  
  if (currentDep === expectedDep) {
    console.log('✅ Test-bench already up-to-date');
    return;
  }
  
  if (!testBench.dependencies) testBench.dependencies = {};
  testBench.dependencies['@continuum/jtag'] = expectedDep;
  
  fs.writeFileSync(testBenchPath, JSON.stringify(testBench, null, 2) + '\n');
  console.log(`✅ Updated test-bench to use ${tarballName}`);
}

function trySmartInstall(): boolean {
  console.log('📥 Trying smart install...');
  
  // Only clean the specific JTAG module
  const jtagPath = 'examples/test-bench/node_modules/@continuum/jtag';
  if (fs.existsSync(jtagPath)) {
    fs.rmSync(jtagPath, { recursive: true, force: true });
  }
  
  try {
    execSync('npm install', { 
      cwd: 'examples/test-bench',
      stdio: 'pipe'
    });
    console.log('✅ Smart install successful');
    return true;
  } catch (error) {
    console.log('⚠️  Smart install failed, trying full clean...');
    return false;
  }
}

function doFullCleanInstall(): void {
  console.log('🧹 Full clean install...');
  
  // Remove everything
  const cleanPaths = [
    'examples/test-bench/node_modules',
    'examples/test-bench/package-lock.json'
  ];
  
  for (const cleanPath of cleanPaths) {
    if (fs.existsSync(cleanPath)) {
      fs.rmSync(cleanPath, { recursive: true, force: true });
    }
  }
  
  execSync('npm install', { 
    cwd: 'examples/test-bench',
    stdio: 'inherit'
  });
  console.log('✅ Full clean install complete');
}

function smartDeploy(): void {
  console.log('🧠 Smart deployment...\n');
  
  const { version, tarballName } = getVersionInfo();
  
  if (!fs.existsSync(tarballName)) {
    console.log('❌ Tarball missing - build first');
    process.exit(1);
  }
  
  console.log(`📦 Version: ${version}`);
  
  updateTestBenchDependency(tarballName);
  
  // Try smart first, fallback to full clean
  if (!trySmartInstall()) {
    doFullCleanInstall();
  }
  
  console.log('\n🎉 Smart deployment complete!');
}

if (require.main === module) {
  smartDeploy();
}

export { smartDeploy };