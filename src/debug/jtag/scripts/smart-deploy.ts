#!/usr/bin/env npx tsx

/**
 * Smart deployment - tries quick install first, falls back to full clean only when needed
 */

import * as fs from 'fs';
import { execSync } from 'child_process';
import { getActiveExample } from '../system/shared/ExampleConfig';

function getVersionInfo() {
  const packageJson = JSON.parse(fs.readFileSync('package.json', 'utf8'));
  const version = packageJson.version;
  const tarballName = `continuum-jtag-${version}.tgz`;
  return { version, tarballName };
}

function updateActiveExampleDependency(tarballName: string): void {
  const activeExample = getActiveExample();
  const examplePath = `${activeExample.paths.directory}/package.json`;
  
  if (!fs.existsSync(examplePath)) {
    console.log(`❌ Active example ${activeExample.name} not found at ${examplePath}`);
    process.exit(1);
  }
  
  const packageJson = JSON.parse(fs.readFileSync(examplePath, 'utf8'));
  const currentDep = packageJson.dependencies?.['@continuum/jtag'];
  const expectedDep = `file:../../${tarballName}`;
  
  if (currentDep === expectedDep) {
    console.log(`✅ ${activeExample.name} already up-to-date`);
    return;
  }
  
  if (!packageJson.dependencies) packageJson.dependencies = {};
  packageJson.dependencies['@continuum/jtag'] = expectedDep;
  
  fs.writeFileSync(examplePath, JSON.stringify(packageJson, null, 2) + '\n');
  console.log(`✅ Updated ${activeExample.name} to use ${tarballName}`);
}

function trySmartInstall(): boolean {
  console.log('📥 Trying smart install...');
  
  const activeExample = getActiveExample();
  const exampleDir = activeExample.paths.directory;
  
  // Only clean the specific JTAG module
  const jtagPath = `${exampleDir}/node_modules/@continuum/jtag`;
  if (fs.existsSync(jtagPath)) {
    fs.rmSync(jtagPath, { recursive: true, force: true });
  }
  
  try {
    console.log('🔄 Running npm install (this may take 10-15 seconds)...');
    execSync('npm install', { 
      cwd: exampleDir,
      stdio: 'inherit', // Show output to prevent timeout confusion
      timeout: 45000 // 45 second timeout for npm install
    });
    console.log(`✅ Smart install successful for ${activeExample.name}`);
    return true;
  } catch (error: any) {
    if (error.signal === 'SIGTERM') {
      console.log(`⚠️  Smart install was terminated (probably by timeout) for ${activeExample.name}`);
    } else {
      console.log(`⚠️  Smart install failed for ${activeExample.name}: ${error.message}`);
    }
    console.log('🔄 Trying full clean install...');
    return false;
  }
}

function doFullCleanInstall(): void {
  console.log('🧹 Full clean install...');
  
  const activeExample = getActiveExample();
  const exampleDir = activeExample.paths.directory;
  
  // Remove everything
  const cleanPaths = [
    `${exampleDir}/node_modules`,
    `${exampleDir}/package-lock.json`
  ];
  
  for (const cleanPath of cleanPaths) {
    if (fs.existsSync(cleanPath)) {
      fs.rmSync(cleanPath, { recursive: true, force: true });
    }
  }
  
  execSync('npm install', { 
    cwd: exampleDir,
    stdio: 'inherit'
  });
  console.log(`✅ Full clean install complete for ${activeExample.name}`);
}

function smartDeploy(): void {
  console.log('🧠 Smart deployment...\n');
  
  const { version, tarballName } = getVersionInfo();
  
  if (!fs.existsSync(tarballName)) {
    console.log('❌ Tarball missing - build first');
    process.exit(1);
  }
  
  console.log(`📦 Version: ${version}`);
  
  updateActiveExampleDependency(tarballName);
  
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