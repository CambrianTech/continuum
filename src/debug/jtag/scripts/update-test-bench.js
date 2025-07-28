#!/usr/bin/env node
/**
 * Update test-bench to use the latest JTAG package version
 * 
 * This script automatically updates the test-bench package.json to point
 * to the most recent JTAG tarball after the build process increments versions.
 */

const fs = require('fs');
const path = require('path');

function updateTestBenchDependency() {
    try {
        // Read the main package.json to get current version
        const mainPackagePath = path.join(__dirname, '..', 'package.json');
        const mainPackage = JSON.parse(fs.readFileSync(mainPackagePath, 'utf8'));
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
        
        // Clean test-bench node_modules to prevent ENOTEMPTY errors
        const testBenchDir = path.join(__dirname, '..', 'examples', 'test-bench');
        const nodeModulesPath = path.join(testBenchDir, 'node_modules');
        
        if (fs.existsSync(nodeModulesPath)) {
            console.log(`🧹 Removing test-bench node_modules to prevent ENOTEMPTY errors...`);
            try {
                fs.rmSync(nodeModulesPath, { recursive: true, force: true });
                console.log(`✅ Test-bench node_modules cleaned`);
            } catch (error) {
                console.warn(`⚠️ Could not remove node_modules: ${error.message}`);
                // Try to remove just the @continuum directory
                const continuumPath = path.join(nodeModulesPath, '@continuum');
                if (fs.existsSync(continuumPath)) {
                    try {
                        fs.rmSync(continuumPath, { recursive: true, force: true });
                        console.log(`✅ Removed @continuum directory specifically`);
                    } catch (nestedError) {
                        console.warn(`⚠️ Could not remove @continuum directory: ${nestedError.message}`);
                    }
                }
            }
        }
        
        // Update test-bench package.json
        const testBenchPackagePath = path.join(__dirname, '..', 'examples', 'test-bench', 'package.json');
        
        if (!fs.existsSync(testBenchPackagePath)) {
            console.error(`❌ Test-bench package.json not found: ${testBenchPackagePath}`);
            process.exit(1);
        }
        
        const testBenchPackage = JSON.parse(fs.readFileSync(testBenchPackagePath, 'utf8'));
        
        // Update the dependency
        const oldDependency = testBenchPackage.dependencies['@continuum/jtag'];
        testBenchPackage.dependencies['@continuum/jtag'] = `file:../../${tarballName}`;
        
        // Write back the updated package.json
        fs.writeFileSync(testBenchPackagePath, JSON.stringify(testBenchPackage, null, 2) + '\n');
        
        console.log(`🔄 Updated test-bench dependency:`);
        console.log(`   From: ${oldDependency}`);
        console.log(`   To:   file:../../${tarballName}`);
        
        // Clean up old tarballs to avoid clutter
        const jtagDir = path.join(__dirname, '..');
        const files = fs.readdirSync(jtagDir);
        const oldTarballs = files.filter(file => 
            file.startsWith('continuum-jtag-') && 
            file.endsWith('.tgz') && 
            file !== tarballName
        );
        
        if (oldTarballs.length > 0) {
            console.log(`🧹 Cleaning up old tarballs:`);
            oldTarballs.forEach(oldTarball => {
                const oldPath = path.join(jtagDir, oldTarball);
                fs.unlinkSync(oldPath);
                console.log(`   🗑️  Removed: ${oldTarball}`);
            });
        }
        
        console.log(`✅ Test-bench dependency updated successfully!`);
        
    } catch (error) {
        console.error(`❌ Failed to update test-bench dependency:`, error.message);
        process.exit(1);
    }
}

updateTestBenchDependency();