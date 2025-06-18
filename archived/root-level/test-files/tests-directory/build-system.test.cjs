
/**
 * NASA-Grade Build System Tests
 * Zero tolerance for build failures
 */

const { exec } = require('child_process');
const { promisify } = require('util');
const fs = require('fs');
const path = require('path');

const execAsync = promisify(exec);

class BuildSystemTestSuite {
  constructor() {
    this.testResults = [];
    this.projectRoot = path.join(__dirname, '..');
  }
  
  async runAllTests() {
    console.log('🔨 Running NASA-grade build system tests...');
    
    await this.testWorkspaceConfiguration();
    await this.testDependencyInstallation();
    await this.testTypeScriptCompilation();
    await this.testLintConfiguration();
    await this.testBuildProcess();
    
    const failedTests = this.testResults.filter(t => !t.passed);
    if (failedTests.length > 0) {
      throw new Error(`BUILD FAILURE: ${failedTests.length} tests failed. Build system must be perfect.`);
    }
    
    console.log(`✅ ALL ${this.testResults.length} build system tests PASSED`);
    return true;
  }
  
  async testWorkspaceConfiguration() {
    const test = { name: 'Workspace Configuration', passed: false, details: [] };
    
    try {
      const packageJsonPath = path.join(this.projectRoot, 'package.json');
      if (fs.existsSync(packageJsonPath)) {
        const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
        
        if (packageJson.workspaces) {
          test.details.push('✅ Workspaces configured');
          test.passed = true;
        } else {
          test.details.push('❌ Workspaces not configured');
        }
      } else {
        test.details.push('❌ package.json missing');
      }
    } catch (error) {
      test.details.push(`❌ Workspace test failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testDependencyInstallation() {
    const test = { name: 'Dependency Installation', passed: false, details: [] };
    
    try {
      const nodeModulesPath = path.join(this.projectRoot, 'node_modules');
      if (fs.existsSync(nodeModulesPath)) {
        test.details.push('✅ node_modules exists');
        test.passed = true;
      } else {
        test.details.push('❌ node_modules missing - running npm install');
        await execAsync('npm install', { cwd: this.projectRoot });
        test.details.push('✅ Dependencies installed');
        test.passed = true;
      }
    } catch (error) {
      test.details.push(`❌ Dependency installation failed: ${error.message}`);
    }
    
    this.testResults.push(test);
  }
  
  async testTypeScriptCompilation() {
    const test = { name: 'TypeScript Compilation', passed: false, details: [] };
    
    try {
      await execAsync('npx tsc --noEmit', { cwd: this.projectRoot });
      test.details.push('✅ TypeScript compilation successful');
      test.passed = true;
    } catch (error) {
      test.details.push(`❌ TypeScript compilation failed: ${error.message.substring(0, 200)}`);
    }
    
    this.testResults.push(test);
  }
  
  async testLintConfiguration() {
    const test = { name: 'Lint Configuration', passed: false, details: [] };
    
    try {
      const result = await execAsync('npm run lint', { cwd: this.projectRoot });
      test.details.push('✅ Linting passed');
      test.passed = true;
    } catch (error) {
      // Try auto-fix
      try {
        await execAsync('npm run lint -- --fix', { cwd: this.projectRoot });
        test.details.push('✅ Linting issues auto-fixed');
        test.passed = true;
      } catch (fixError) {
        test.details.push(`❌ Linting failed: ${error.message.substring(0, 200)}`);
      }
    }
    
    this.testResults.push(test);
  }
  
  async testBuildProcess() {
    const test = { name: 'Build Process', passed: false, details: [] };
    
    try {
      await execAsync('npm run build', { cwd: this.projectRoot });
      test.details.push('✅ Build process successful');
      test.passed = true;
    } catch (error) {
      test.details.push(`❌ Build process failed: ${error.message.substring(0, 200)}`);
    }
    
    this.testResults.push(test);
  }
}

module.exports = BuildSystemTestSuite;
