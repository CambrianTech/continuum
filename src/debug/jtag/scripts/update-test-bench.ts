#!/usr/bin/env npx tsx

/**
 * Update test-bench to use the latest JTAG package version
 * 
 * This script automatically updates the test-bench package.json to point
 * to the most recent JTAG tarball after the build process increments versions.
 */

import * as fs from 'fs';
import * as path from 'path';

interface PackageJson {
  version: string;
  dependencies?: Record<string, string>;
  [key: string]: any;
}

function updateTestBenchDependency(): void {
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
        const testBenchPackagePath = path.join(__dirname, '..', 'examples', 'test-bench', 'package.json');
        
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
        
    } catch (error) {
        console.error(`❌ Failed to update test bench: ${error}`);
        process.exit(1);
    }
}

// Run if called directly
if (require.main === module) {
    updateTestBenchDependency();
}

export { updateTestBenchDependency };