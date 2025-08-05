#!/usr/bin/env npx tsx

/**
 * Deploy JTAG test-bench with clean installation
 * 
 * This script handles the complete deployment process:
 * 1. Updates test-bench package.json to use latest JTAG tarball
 * 2. Cleans node_modules and package-lock.json to avoid conflicts
 * 3. Performs fresh npm install
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

interface PackageJson {
  version: string;
  dependencies?: Record<string, string>;
  [key: string]: any;
}

function deployTestBench(): void {
    try {
        // Read the main package.json to get current version
        const mainPackagePath = path.join(__dirname, '..', 'package.json');
        const mainPackage: PackageJson = JSON.parse(fs.readFileSync(mainPackagePath, 'utf8'));
        const currentVersion = mainPackage.version;
        
        console.log(`📦 Current JTAG version: ${currentVersion}`);
        
        // Check if the corresponding tarball exists
        const tarballName = `continuum-jtag-${currentVersion}.tgz`;
        const tarballPath = path.join(__dirname, '..', tarballName);
        
        if (!fs.existsSync(tarballPath)) {
            console.error(`❌ Tarball not found: ${tarballPath}`);
            process.exit(1);
        }
        
        console.log(`✅ Found tarball: ${tarballName}`);
        
        // Update test-bench package.json
        const testBenchDir = path.join(__dirname, '..', 'examples', 'test-bench');
        const testBenchPackagePath = path.join(testBenchDir, 'package.json');
        
        if (!fs.existsSync(testBenchPackagePath)) {
            console.error(`❌ Test bench package.json not found: ${testBenchPackagePath}`);
            process.exit(1);
        }
        
        const testBenchPackage: PackageJson = JSON.parse(fs.readFileSync(testBenchPackagePath, 'utf8'));
        
        // Update the dependency to point to the local tarball
        if (!testBenchPackage.dependencies) {
            testBenchPackage.dependencies = {};
        }
        
        testBenchPackage.dependencies['@continuum/jtag'] = `file:../../${tarballName}`;
        
        // Write back the updated package.json
        fs.writeFileSync(testBenchPackagePath, JSON.stringify(testBenchPackage, null, 2) + '\n');
        
        console.log(`✅ Test bench updated to use JTAG v${currentVersion}`);
        console.log(`   Dependency: @continuum/jtag → file:../../${tarballName}`);
        
        // Clean and reinstall dependencies
        console.log(`🧹 Cleaning test-bench dependencies...`);
        
        const nodeModulesPath = path.join(testBenchDir, 'node_modules');
        const packageLockPath = path.join(testBenchDir, 'package-lock.json');
        
        // Remove node_modules and package-lock.json if they exist
        if (fs.existsSync(nodeModulesPath)) {
            fs.rmSync(nodeModulesPath, { recursive: true, force: true });
            console.log(`   Removed node_modules`);
        }
        
        if (fs.existsSync(packageLockPath)) {
            fs.unlinkSync(packageLockPath);
            console.log(`   Removed package-lock.json`);
        }
        
        // Fresh npm install
        console.log(`📥 Installing dependencies...`);
        execSync('npm install', { 
            cwd: testBenchDir, 
            stdio: 'inherit' 
        });
        
        console.log(`🎉 Test bench deployment complete!`);
        
    } catch (error) {
        console.error(`❌ Failed to deploy test bench: ${error}`);
        process.exit(1);
    }
}

// Run if called directly
if (require.main === module) {
    deployTestBench();
}

export { deployTestBench };