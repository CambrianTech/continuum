/**
 * Modular Test Runner - Modern TypeScript Test Discovery and Execution
 * Automatically discovers and runs tests within each module (daemons, widgets, commands)
 */

import { promises as fs } from 'fs';
import { join, dirname, relative } from 'path';
import { spawn } from 'child_process';

interface TestModule {
  path: string;
  name: string;
  type: 'daemon' | 'widget' | 'command' | 'integration';
  testFiles: string[];
}

interface TestResult {
  module: string;
  testFile: string;
  passed: boolean;
  duration: number;
  output: string;
  error?: string;
}

export class ModularTestRunner {
  private projectRoot: string;
  private testModules: TestModule[] = [];
  
  constructor(projectRoot: string = process.cwd()) {
    this.projectRoot = projectRoot;
  }

  /**
   * Discover all test modules throughout the project
   */
  async discoverTests(): Promise<TestModule[]> {
    console.log('🔍 Discovering test modules...');
    
    const testModules: TestModule[] = [];
    
    // Scan for daemon tests
    const daemonTests = await this.scanDirectory('src/daemons', 'daemon');
    testModules.push(...daemonTests);
    
    // Scan for widget tests  
    const widgetTests = await this.scanDirectory('src/ui/components', 'widget');
    testModules.push(...widgetTests);
    
    // Scan for command tests
    const commandTests = await this.scanDirectory('src/commands', 'command');
    testModules.push(...commandTests);
    
    // Scan for integration tests
    const integrationTests = await this.scanDirectory('src/integrations', 'integration');
    testModules.push(...integrationTests);
    
    this.testModules = testModules;
    console.log(`✅ Discovered ${testModules.length} test modules`);
    
    return testModules;
  }

  /**
   * Scan a directory for modules containing test files
   */
  private async scanDirectory(dirPath: string, type: TestModule['type']): Promise<TestModule[]> {
    const fullPath = join(this.projectRoot, dirPath);
    const modules: TestModule[] = [];
    
    try {
      const entries = await fs.readdir(fullPath, { withFileTypes: true });
      
      for (const entry of entries) {
        if (entry.isDirectory()) {
          const modulePath = join(fullPath, entry.name);
          const testFiles = await this.findTestFiles(modulePath);
          
          if (testFiles.length > 0) {
            modules.push({
              path: modulePath,
              name: entry.name,
              type,
              testFiles
            });
          }
        }
      }
    } catch (error) {
      // Directory doesn't exist, skip silently
    }
    
    return modules;
  }

  /**
   * Find all test files in a module directory
   */
  private async findTestFiles(modulePath: string): Promise<string[]> {
    const testFiles: string[] = [];
    
    try {
      const entries = await fs.readdir(modulePath, { withFileTypes: true });
      
      for (const entry of entries) {
        const entryPath = join(modulePath, entry.name);
        
        if (entry.isDirectory()) {
          // Recursively scan subdirectories (like test/ folders)
          const subTests = await this.findTestFiles(entryPath);
          testFiles.push(...subTests);
        } else if (this.isTestFile(entry.name)) {
          testFiles.push(entryPath);
        }
      }
    } catch (error) {
      // Skip directories we can't read
    }
    
    return testFiles;
  }

  /**
   * Check if a file is a test file
   */
  private isTestFile(filename: string): boolean {
    return (
      filename.endsWith('.test.ts') ||
      filename.endsWith('.test.js') ||
      filename.endsWith('.spec.ts') ||
      filename.endsWith('.spec.js')
    ) && !filename.includes('.d.ts');
  }

  /**
   * Run all discovered tests
   */
  async runAllTests(): Promise<TestResult[]> {
    if (this.testModules.length === 0) {
      await this.discoverTests();
    }
    
    console.log(`🧪 Running tests for ${this.testModules.length} modules...`);
    
    const allResults: TestResult[] = [];
    
    for (const module of this.testModules) {
      console.log(`\n📦 Testing module: ${module.name} (${module.type})`);
      
      for (const testFile of module.testFiles) {
        const result = await this.runTestFile(module, testFile);
        allResults.push(result);
        
        if (result.passed) {
          console.log(`  ✅ ${relative(module.path, testFile)} (${result.duration}ms)`);
        } else {
          console.log(`  ❌ ${relative(module.path, testFile)} (${result.duration}ms)`);
          if (result.error) {
            console.log(`     Error: ${result.error}`);
          }
        }
      }
    }
    
    return allResults;
  }

  /**
   * Run tests for a specific module type
   */
  async runModuleTypeTests(type: TestModule['type']): Promise<TestResult[]> {
    if (this.testModules.length === 0) {
      await this.discoverTests();
    }
    
    const moduleTests = this.testModules.filter(m => m.type === type);
    console.log(`🧪 Running ${type} tests for ${moduleTests.length} modules...`);
    
    const results: TestResult[] = [];
    
    for (const module of moduleTests) {
      console.log(`\n📦 Testing ${type}: ${module.name}`);
      
      for (const testFile of module.testFiles) {
        const result = await this.runTestFile(module, testFile);
        results.push(result);
        
        if (result.passed) {
          console.log(`  ✅ ${relative(module.path, testFile)} (${result.duration}ms)`);
        } else {
          console.log(`  ❌ ${relative(module.path, testFile)} (${result.duration}ms)`);
        }
      }
    }
    
    return results;
  }

  /**
   * Run a specific test file
   */
  private async runTestFile(module: TestModule, testFile: string): Promise<TestResult> {
    const startTime = Date.now();
    const relativeTestFile = relative(this.projectRoot, testFile);
    
    try {
      // Use Jest for TypeScript test files
      if (testFile.endsWith('.ts')) {
        return await this.runJestTest(module, testFile, startTime);
      } else {
        return await this.runNodeTest(module, testFile, startTime);
      }
    } catch (error) {
      return {
        module: module.name,
        testFile: relativeTestFile,
        passed: false,
        duration: Date.now() - startTime,
        output: '',
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Run TypeScript test with Jest
   */
  private async runJestTest(module: TestModule, testFile: string, startTime: number): Promise<TestResult> {
    const relativeTestFile = relative(this.projectRoot, testFile);
    
    return new Promise((resolve) => {
      const jestProcess = spawn('npx', ['jest', testFile, '--testTimeout=30000'], {
        cwd: this.projectRoot,
        stdio: ['ignore', 'pipe', 'pipe']
      });
      
      let output = '';
      let error = '';
      
      jestProcess.stdout?.on('data', (data) => {
        output += data.toString();
      });
      
      jestProcess.stderr?.on('data', (data) => {
        error += data.toString();
      });
      
      jestProcess.on('close', (code) => {
        const duration = Date.now() - startTime;
        
        resolve({
          module: module.name,
          testFile: relativeTestFile,
          passed: code === 0,
          duration,
          output,
          error: code !== 0 ? error : undefined
        });
      });
      
      // Timeout after 60 seconds
      setTimeout(() => {
        jestProcess.kill();
        resolve({
          module: module.name,
          testFile: relativeTestFile,
          passed: false,
          duration: Date.now() - startTime,
          output,
          error: 'Test timeout (60s)'
        });
      }, 60000);
    });
  }

  /**
   * Run JavaScript test with Node
   */
  private async runNodeTest(module: TestModule, testFile: string, startTime: number): Promise<TestResult> {
    const relativeTestFile = relative(this.projectRoot, testFile);
    
    return new Promise((resolve) => {
      const nodeProcess = spawn('node', [testFile], {
        cwd: dirname(testFile),
        stdio: ['ignore', 'pipe', 'pipe']
      });
      
      let output = '';
      let error = '';
      
      nodeProcess.stdout?.on('data', (data) => {
        output += data.toString();
      });
      
      nodeProcess.stderr?.on('data', (data) => {
        error += data.toString();
      });
      
      nodeProcess.on('close', (code) => {
        const duration = Date.now() - startTime;
        
        resolve({
          module: module.name,
          testFile: relativeTestFile,
          passed: code === 0,
          duration,
          output,
          error: code !== 0 ? error : undefined
        });
      });
    });
  }

  /**
   * Generate test report
   */
  generateReport(results: TestResult[]): void {
    const totalTests = results.length;
    const passedTests = results.filter(r => r.passed).length;
    const failedTests = totalTests - passedTests;
    const totalDuration = results.reduce((sum, r) => sum + r.duration, 0);
    
    console.log('\n📊 Test Report');
    console.log('='.repeat(50));
    console.log(`Total tests: ${totalTests}`);
    console.log(`Passed: ${passedTests}`);
    console.log(`Failed: ${failedTests}`);
    console.log(`Success rate: ${Math.round((passedTests / totalTests) * 100)}%`);
    console.log(`Total duration: ${totalDuration}ms`);
    
    if (failedTests > 0) {
      console.log('\n❌ Failed tests:');
      results.filter(r => !r.passed).forEach(result => {
        console.log(`  - ${result.module}/${result.testFile}`);
        if (result.error) {
          console.log(`    ${result.error}`);
        }
      });
    }
    
    // Group by module type
    const byType = results.reduce((acc, result) => {
      const module = this.testModules.find(m => m.name === result.module);
      const type = module?.type || 'unknown';
      if (!acc[type]) acc[type] = [];
      acc[type].push(result);
      return acc;
    }, {} as Record<string, TestResult[]>);
    
    console.log('\n📦 Results by module type:');
    Object.entries(byType).forEach(([type, typeResults]) => {
      const typePassed = typeResults.filter(r => r.passed).length;
      const typeTotal = typeResults.length;
      console.log(`  ${type}: ${typePassed}/${typeTotal} passed`);
    });
  }
}

// CLI interface when run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const runner = new ModularTestRunner();
  const args = process.argv.slice(2);
  
  async function main() {
    try {
      let results: TestResult[];
      
      if (args.length === 0) {
        // Run all tests
        results = await runner.runAllTests();
      } else {
        const testType = args[0] as TestModule['type'];
        if (['daemon', 'widget', 'command', 'integration'].includes(testType)) {
          results = await runner.runModuleTypeTests(testType);
        } else {
          console.error(`❌ Invalid test type: ${testType}`);
          console.error('Valid types: daemon, widget, command, integration');
          process.exit(1);
        }
      }
      
      runner.generateReport(results);
      
      const failedTests = results.filter(r => !r.passed).length;
      process.exit(failedTests > 0 ? 1 : 0);
      
    } catch (error) {
      console.error('❌ Test runner failed:', error);
      process.exit(1);
    }
  }
  
  main();
}