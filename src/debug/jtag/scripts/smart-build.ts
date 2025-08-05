#!/usr/bin/env npx tsx

/**
 * Smart build script that only recompiles when needed
 */

import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import { execSync } from 'child_process';
import { globSync } from 'glob';

interface BuildCheck {
  name: string;
  needed: boolean;
  reason: string;
}

function getFileModTime(filePath: string): number {
  try {
    return fs.statSync(filePath).mtime.getTime();
  } catch {
    return 0;
  }
}

function getNewestFileTime(pattern: string): number {
  try {
    const files = globSync(pattern);
    if (files.length === 0) return 0;
    return Math.max(...files.map(getFileModTime));
  } catch {
    return 0;
  }
}

function checkTypeScriptBuild(): BuildCheck {
  const tsFiles = getNewestFileTime('**/*.ts');
  const jsFiles = getNewestFileTime('dist/**/*.js');
  const tsConfig = getFileModTime('tsconfig.json');
  
  if (jsFiles === 0) {
    return { name: 'TypeScript', needed: true, reason: 'No compiled JavaScript files found' };
  }
  if (tsFiles > jsFiles) {
    return { name: 'TypeScript', needed: true, reason: 'TypeScript source files newer than compiled output' };
  }
  if (tsConfig > jsFiles) {
    return { name: 'TypeScript', needed: true, reason: 'tsconfig.json modified since last build' };
  }
  return { name: 'TypeScript', needed: false, reason: 'All TypeScript files up to date' };
}

function getContentHash(content: string): string {
  // Remove timestamp lines and normalize whitespace for content comparison
  const normalized = content
    .replace(/Auto-generated on \d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z/, 'TIMESTAMP_PLACEHOLDER')
    .replace(/\s+/g, ' ')
    .trim();
  return crypto.createHash('md5').update(normalized).digest('hex');
}

function getFileContentHash(filePath: string): string {
  try {
    const content = fs.readFileSync(filePath, 'utf8');
    return getContentHash(content);
  } catch {
    return '';
  }
}

function checkGeneratedFiles(): BuildCheck {
  const sourceFiles = getNewestFileTime('{daemons,commands,system}/**/*.ts');
  const generatedBrowser = getFileModTime('browser/generated.ts');
  const generatedServer = getFileModTime('server/generated.ts');
  const packageJson = getFileModTime('package.json');
  
  if (generatedBrowser === 0 || generatedServer === 0) {
    return { name: 'Generated files', needed: true, reason: 'Generated files missing' };
  }
  
  // Check if source files changed since generation
  const oldestGenerated = Math.min(generatedBrowser, generatedServer);
  if (sourceFiles > oldestGenerated) {
    return { name: 'Generated files', needed: true, reason: 'Source files newer than generated files' };
  }
  
  // Check if package.json structure changed since generation
  if (packageJson > oldestGenerated) {
    // But only if the structureGenerator config actually changed
    try {
      const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
      const structureGen = pkg.structureGenerator;
      const configHash = crypto.createHash('md5').update(JSON.stringify(structureGen || {})).digest('hex');
      
      // Store and compare config hash to detect real changes
      const hashFile = '.continuum/generator/structure-config.hash';
      let lastConfigHash = '';
      try {
        lastConfigHash = fs.readFileSync(hashFile, 'utf8').trim();
      } catch {}
      
      if (configHash !== lastConfigHash) {
        // Config actually changed - need rebuild
        fs.mkdirSync(path.dirname(hashFile), { recursive: true });
        fs.writeFileSync(hashFile, configHash);
        return { name: 'Generated files', needed: true, reason: 'Structure generator configuration changed' };
      }
    } catch {
      // If we can't determine config changes, assume no change needed
    }
  }
  
  return { name: 'Generated files', needed: false, reason: 'Generated files up to date' };
}

function checkTarball(): BuildCheck {
  const packageJson = JSON.parse(fs.readFileSync('package.json', 'utf8'));
  const version = packageJson.version;
  const tarballName = `continuum-jtag-${version}.tgz`;
  
  const tarballTime = getFileModTime(tarballName);
  const distFiles = getNewestFileTime('dist/**/*');
  const packageTime = getFileModTime('package.json');
  
  if (tarballTime === 0) {
    return { name: 'Tarball', needed: true, reason: 'Tarball does not exist' };
  }
  if (distFiles > tarballTime || packageTime > tarballTime) {
    return { name: 'Tarball', needed: true, reason: 'Built files newer than tarball' };
  }
  return { name: 'Tarball', needed: false, reason: 'Tarball up to date' };
}

function runBuildStep(stepName: string, command: string): void {
  console.log(`🔨 Running ${stepName}...`);
  try {
    execSync(command, { stdio: 'inherit' });
    console.log(`✅ ${stepName} complete`);
  } catch (error) {
    console.error(`❌ ${stepName} failed:`, error);
    process.exit(1);
  }
}

function smartBuild(): void {
  console.log('🧠 Smart build analysis...\n');
  
  const checks: BuildCheck[] = [
    checkGeneratedFiles(),
    checkTypeScriptBuild(),
    checkTarball()
  ];
  
  for (const check of checks) {
    const status = check.needed ? '🔴 REBUILD NEEDED' : '✅ UP TO DATE';
    console.log(`${status} ${check.name}: ${check.reason}`);
  }
  
  const rebuildsNeeded = checks.filter(c => c.needed);
  
  if (rebuildsNeeded.length === 0) {
    console.log('\n🎉 Everything is up to date! No build needed.');
    return;
  }
  
  console.log(`\n🔨 ${rebuildsNeeded.length} build step(s) needed:`);
  rebuildsNeeded.forEach(c => console.log(`   • ${c.name}`));
  
  for (const check of checks) {
    if (!check.needed) continue;
    
    switch (check.name) {
      case 'Generated files':
        runBuildStep('Structure generation', 'npm run prebuild');
        break;
      case 'TypeScript':
        runBuildStep('TypeScript compilation', 'npm run build:ts');
        runBuildStep('Post-build processing', 'npm run postbuild');
        break;
      case 'Tarball':
        runBuildStep('Package creation', 'npm run pack');
        break;
    }
  }
  
  console.log('\n🎉 Smart build complete!');
}

if (require.main === module) {
  smartBuild();
}

export { smartBuild };