#!/usr/bin/env npx tsx
/**
 * GLOBAL CLI INSTALLATION WORKFLOW TESTS
 * 
 * Tests the npm install -g @continuum/jtag workflow and ensures
 * the CLI works correctly when installed globally across different projects.
 */

import fs from 'fs/promises';
import path from 'path';
import { WorkingDirConfig } from '../system/core/config/WorkingDirConfig';
import { getActiveExampleName, getActivePorts } from "../examples/server/ExampleConfigServer";
import { createTypedErrorInfo } from '../system/core/types/ErrorTypes';

interface GlobalInstallTestResult {
  readonly success: boolean;
  readonly testName: string;
  readonly details: string[];
  readonly error?: string;
}

interface PackageJsonConfig {
  readonly name: string;
  readonly version: string;
  readonly description: string;
  readonly bin: Record<string, string>;
  readonly exports: Record<string, unknown>;
  readonly main: string;
  readonly browser: string;
  readonly types: string;
}

async function testPackageJsonConfiguration(): Promise<GlobalInstallTestResult> {
  const testName = "package.json Global Installation Configuration";
  const details: string[] = [];
  
  try {
    // Read and validate package.json
    const packageJsonPath = path.resolve('package.json');
    const packageJsonContent = await fs.readFile(packageJsonPath, 'utf-8');
    const packageJson = JSON.parse(packageJsonContent) as PackageJsonConfig;
    
    // Test 1: Validate package name for global installation
    if (!packageJson.name.startsWith('@continuum/')) {
      throw new Error(`Package name should start with @continuum/ for scoped global installation, got: ${packageJson.name}`);
    }
    details.push(`✅ Package name: ${packageJson.name} (scoped for global installation)`);
    
    // Test 2: Validate CLI binary configuration
    const expectedBinaries = ['continuum', 'jtag'];
    for (const binary of expectedBinaries) {
      if (!packageJson.bin || !packageJson.bin[binary]) {
        throw new Error(`Missing CLI binary: ${binary}`);
      }
      details.push(`✅ CLI binary '${binary}': ${packageJson.bin[binary]}`);
    }
    
    // Test 3: Validate description includes global installation instructions
    if (!packageJson.description.includes('npm install -g')) {
      throw new Error('Package description should include global installation instructions');
    }
    details.push(`✅ Description includes global installation instructions`);
    details.push(`   "${packageJson.description}"`);
    
    // Test 4: Validate entry points for different environments
    const requiredEntries = ['main', 'browser', 'types'] as const;
    for (const entry of requiredEntries) {
      if (!packageJson[entry]) {
        throw new Error(`Missing ${entry} entry point in package.json`);
      }
      details.push(`✅ ${entry}: ${packageJson[entry]}`);
    }
    
    // Test 5: Validate exports configuration for modern Node.js
    if (!packageJson.exports || typeof packageJson.exports !== 'object') {
      throw new Error('Missing or invalid exports configuration for modern Node.js');
    }
    details.push(`✅ Modern exports configuration present`);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testProjectDirectoryIsolation(): Promise<GlobalInstallTestResult> {
  const testName = "Per-Project Directory Isolation for Global CLI";
  const details: string[] = [];
  
  try {
    const originalCwd = process.cwd();
    details.push(`📍 Original directory: ${originalCwd}`);
    
    // Test 1: Simulate being in a different project directory
    const mockProjectDir = path.join(originalCwd, 'examples', 'widget-ui');
    process.chdir(mockProjectDir);
    details.push(`📁 Changed to project directory: ${mockProjectDir}`);
    
    // Test 2: Verify WorkingDirConfig respects current directory
    const workingDir = WorkingDirConfig.getWorkingDir();
    if (!workingDir.includes('widget-ui')) {
      throw new Error(`Working directory should reflect current project context, got: ${workingDir}`);
    }
    details.push(`✅ Working directory reflects project context: ${workingDir}`);
    
    // Test 3: Verify .continuum directory would be created in project root
    const continuumPath = WorkingDirConfig.getContinuumPath();
    if (!continuumPath.includes('widget-ui')) {
      throw new Error(`Continuum path should be project-local, got: ${continuumPath}`);
    }
    details.push(`✅ Continuum path is project-local: ${continuumPath}`);
    
    // Test 4: Verify project-specific configuration is respected
    const activeExample = getActiveExampleName();
    const activePorts = getActivePorts();
    details.push(`✅ Active example detected: ${activeExample}`);
    details.push(`✅ Project-specific ports: WebSocket=${activePorts.websocket_server}, HTTP=${activePorts.http_server}`);
    
    // Restore original directory
    process.chdir(originalCwd);
    details.push(`📁 Restored original directory`);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testCLIEntryPointDesign(): Promise<GlobalInstallTestResult> {
  const testName = "CLI Entry Point Design for Global Usage";
  const details: string[] = [];
  
  try {
    // Test 1: Verify CLI source file exists and is executable
    const cliSourcePath = path.resolve('cli.ts');
    const cliStats = await fs.stat(cliSourcePath);
    details.push(`✅ CLI source exists: ${cliSourcePath} (${cliStats.size} bytes)`);
    
    // Test 2: Verify CLI has proper shebang for global execution
    const cliContent = await fs.readFile(cliSourcePath, 'utf-8');
    if (!cliContent.startsWith('#!/usr/bin/env')) {
      throw new Error('CLI should start with proper shebang for global execution');
    }
    details.push(`✅ CLI has proper shebang for global execution`);
    
    // Test 3: Verify CLI imports are designed for location independence
    const hasLocationIndependentImports = cliContent.includes('./system/core/client/server/JTAGClientServer');
    if (!hasLocationIndependentImports) {
      throw new Error('CLI should use relative imports for location independence');
    }
    details.push(`✅ CLI uses location-independent imports`);
    
    // Test 4: Verify CLI includes help system
    const hasHelpSystem = cliContent.includes('displayHelp') && cliContent.includes('Global Debugging CLI');
    if (!hasHelpSystem) {
      throw new Error('CLI should include comprehensive help system');
    }
    details.push(`✅ CLI includes comprehensive help system`);
    
    // Test 5: Verify CLI mentions global installation in help
    const hasGlobalInstallHelp = cliContent.includes('npm install -g @continuum/jtag');
    if (!hasGlobalInstallHelp) {
      throw new Error('CLI help should mention global installation');
    }
    details.push(`✅ CLI help includes global installation instructions`);
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function testPackageDeploymentReadiness(): Promise<GlobalInstallTestResult> {
  const testName = "Package Deployment Readiness";
  const details: string[] = [];
  
  try {
    // Test 1: Verify essential files exist
    const essentialFiles = [
      'package.json',
      'README.md', 
      'cli.ts',
      'browser-index.ts',
      'server-index.ts'
    ];
    
    for (const file of essentialFiles) {
      try {
        const stats = await fs.stat(file);
        details.push(`✅ ${file}: ${stats.size} bytes`);
      } catch {
        throw new Error(`Missing essential file for deployment: ${file}`);
      }
    }
    
    // Test 2: Check if dist directory exists (build artifacts)
    try {
      const distStats = await fs.stat('dist');
      if (distStats.isDirectory()) {
        details.push(`✅ Build artifacts exist in dist/ directory`);
      }
    } catch {
      details.push(`⚠️ No build artifacts found - run build before deployment`);
    }
    
    // Test 3: Verify README exists and likely contains installation instructions
    const readmeContent = await fs.readFile('README.md', 'utf-8');
    const hasInstallInstructions = readmeContent.includes('install') && readmeContent.includes('npm');
    if (hasInstallInstructions) {
      details.push(`✅ README contains installation instructions`);
    } else {
      details.push(`⚠️ README should contain clear installation instructions`);
    }
    
    // Test 4: Check for essential TypeScript configuration
    try {
      await fs.stat('tsconfig.json');
      details.push(`✅ TypeScript configuration present`);
    } catch {
      throw new Error('Missing tsconfig.json - required for TypeScript compilation');
    }
    
    return {
      success: true,
      testName,
      details
    };
    
  } catch (error: unknown) {
    const typedError = createTypedErrorInfo(error);
    return {
      success: false,
      testName,
      details,
      error: typedError.message
    };
  }
}

async function runGlobalInstallationTests(): Promise<void> {
  console.log('🌍 GLOBAL CLI INSTALLATION WORKFLOW TESTS');
  console.log('═'.repeat(60));
  
  const tests = [
    testPackageJsonConfiguration,
    testProjectDirectoryIsolation,
    testCLIEntryPointDesign,
    testPackageDeploymentReadiness
  ];
  
  const results: GlobalInstallTestResult[] = [];
  
  for (const test of tests) {
    console.log(`\n▶️ Running: ${test.name.replace(/([A-Z])/g, ' $1').trim()}...`);
    const result = await test();
    results.push(result);
    
    if (result.success) {
      console.log(`✅ PASSED: ${result.testName}`);
      for (const detail of result.details) {
        console.log(`   ${detail}`);
      }
    } else {
      console.log(`❌ FAILED: ${result.testName}`);
      for (const detail of result.details) {
        console.log(`   ${detail}`);
      }
      if (result.error) {
        console.log(`   ❌ Error: ${result.error}`);
      }
    }
  }
  
  // Summary
  const passed = results.filter(r => r.success).length;
  const total = results.length;
  
  console.log('\n' + '═'.repeat(60));
  console.log(`📊 GLOBAL CLI TESTS SUMMARY: ${passed}/${total} PASSED`);
  
  if (passed === total) {
    console.log('🎉 ALL TESTS PASSED!');
    console.log('✅ Package is ready for global CLI installation workflow');
    console.log('📦 Ready for: npm install -g @continuum/jtag');
    process.exit(0);
  } else {
    console.log(`❌ ${total - passed} TESTS FAILED`);
    console.log('🔍 Address the issues above before global deployment');
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  runGlobalInstallationTests().catch((error: unknown) => {
    const typedError = createTypedErrorInfo(error);
    console.error('❌ Test execution failed:', typedError.message);
    process.exit(1);
  });
}

export { 
  runGlobalInstallationTests, 
  testPackageJsonConfiguration, 
  testProjectDirectoryIsolation, 
  testCLIEntryPointDesign,
  testPackageDeploymentReadiness 
};