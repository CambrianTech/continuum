#!/usr/bin/env tsx
/**
 * MODULAR TEST RUNNER - Middle-Out Testing for Widgets and Daemons
 * 
 * Runs specific test categories:
 * - widget: Tests all widget compilation, asset loading, and functionality
 * - daemon: Tests all daemon modules, discovery, and communication
 * 
 * Usage:
 *   npm run test:widgets  (runs: npx tsx src/testing/ModularTestRunner.ts widget)
 *   npm run test:daemons  (runs: npx tsx src/testing/ModularTestRunner.ts daemon)
 */

import { AllWidgetsTestRunner } from '../ui/components/test/AllWidgetsTest';
import * as path from 'path';
import * as fs from 'fs';

interface TestResult {
  category: string;
  testName: string;
  passed: boolean;
  error?: string;
  duration: number;
}

class ModularTestRunner {
  private rootDir: string;
  private results: TestResult[] = [];

  constructor() {
    this.rootDir = this.findProjectRoot();
  }

  private findProjectRoot(): string {
    let currentDir = path.dirname(new URL(import.meta.url).pathname);
    
    while (currentDir !== path.dirname(currentDir)) {
      const packagePath = path.join(currentDir, 'package.json');
      if (fs.existsSync(packagePath)) {
        return currentDir;
      }
      currentDir = path.dirname(currentDir);
    }
    
    return path.resolve(process.cwd());
  }

  async runTests(category: string): Promise<void> {
    console.log(`🧪 MODULAR TEST RUNNER: ${category.toUpperCase()} TESTING`);
    console.log(`════════════════════════════════════════════════`);
    
    const startTime = Date.now();
    
    try {
      switch (category.toLowerCase()) {
        case 'widget':
        case 'widgets':
          await this.runWidgetTests();
          break;
          
        case 'daemon':
        case 'daemons':
          await this.runDaemonTests();
          break;
          
        default:
          throw new Error(`Unknown test category: ${category}. Use 'widget' or 'daemon'`);
      }
      
      this.printResults();
      
    } catch (error) {
      console.error(`❌ Test runner failed:`, error);
      process.exit(1);
    }
    
    const duration = Date.now() - startTime;
    console.log(`⏱️  Total duration: ${duration}ms`);
    
    // Exit with appropriate code
    const failures = this.results.filter(r => !r.passed);
    if (failures.length > 0) {
      console.log(`❌ ${failures.length} test(s) failed`);
      process.exit(1);
    } else {
      console.log(`✅ All tests passed!`);
      process.exit(0);
    }
  }

  private async runWidgetTests(): Promise<void> {
    console.log(`🎨 WIDGET TESTING - Middle-Out Layer 5`);
    console.log(`──────────────────────────────────────`);
    
    try {
      // Test 1: Widget Discovery and Compilation
      await this.runTest('widget', 'Widget Discovery', async () => {
        const widgetTestRunner = new AllWidgetsTestRunner();
        const results = await widgetTestRunner.runAllWidgetTests();
        
        console.log(`📦 Discovered ${results.totalWidgets} widgets (${results.compliantWidgets} compliant)`);
        
        if (results.summary.failed > 0) {
          throw new Error(`${results.summary.failed} widget tests failed out of ${results.summary.totalTests}`);
        }
        
        console.log(`✅ All widget tests passed: ${results.summary.passed}/${results.summary.totalTests}`);
        return true;
      });

      // Test 2: Asset Loading Validation
      await this.runTest('widget', 'Asset Loading', async () => {
        console.log(`🎯 Testing widget asset loading...`);
        
        // Import UniversalWidgetAssetTester
        const { UniversalWidgetAssetTester } = await import('../ui/components/shared/test/UniversalWidgetAssetTest');
        const _assetTester = new UniversalWidgetAssetTester();
        void _assetTester; // Available for future use
        
        // Test core widgets
        const coreWidgets = [
          '../ui/components/Chat/ChatWidget.js',
          '../ui/components/Sidebar/SidebarWidget.js',
          '../ui/components/UserSelector/UserSelector.js',
          '../ui/components/Academy/AcademyStatusWidget.js'
        ];
        
        let allAssetsPassed = true;
        for (const widgetPath of coreWidgets) {
          try {
            const WidgetClass = (await import(widgetPath)).default || (await import(widgetPath))[Object.keys(await import(widgetPath))[0]];
            if (WidgetClass && typeof WidgetClass.getWidgetAssets === 'function') {
              const assets = await WidgetClass.getWidgetAssets();
              console.log(`📄 ${WidgetClass.name}: ${assets.length} assets declared`);
            }
          } catch (error) {
            console.warn(`⚠️  Could not test assets for ${widgetPath}:`, error);
            allAssetsPassed = false;
          }
        }
        
        return allAssetsPassed;
      });

    } catch (error) {
      console.error(`❌ Widget testing failed:`, error);
      throw error;
    }
  }

  private async runDaemonTests(): Promise<void> {
    console.log(`⚙️  DAEMON TESTING - Middle-Out Layer 2`);
    console.log(`──────────────────────────────────────`);
    
    try {
      // Test 1: Daemon Module Discovery
      await this.runTest('daemon', 'Module Discovery', async () => {
        const daemonsDir = path.join(this.rootDir, 'src', 'daemons');
        
        if (!fs.existsSync(daemonsDir)) {
          throw new Error(`Daemons directory not found: ${daemonsDir}`);
        }
        
        const daemonDirs = fs.readdirSync(daemonsDir, { withFileTypes: true })
          .filter(dirent => dirent.isDirectory())
          .map(dirent => dirent.name);
        
        console.log(`📦 Found ${daemonDirs.length} daemon modules:`);
        
        let allValid = true;
        for (const daemonDir of daemonDirs) {
          const packageJsonPath = path.join(daemonsDir, daemonDir, 'package.json');
          
          if (fs.existsSync(packageJsonPath)) {
            try {
              const packageData = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
              if (packageData.continuum?.type === 'daemon') {
                console.log(`✅ ${daemonDir}: Valid daemon module`);
              } else {
                console.log(`⚠️  ${daemonDir}: Missing continuum.type='daemon' in package.json`);
                allValid = false;
              }
            } catch (error) {
              console.log(`❌ ${daemonDir}: Invalid package.json`);
              allValid = false;
            }
          } else {
            console.log(`❌ ${daemonDir}: Missing package.json`);
            allValid = false;
          }
        }
        
        return allValid;
      });

      // Test 2: TypeScript Compilation
      await this.runTest('daemon', 'TypeScript Compilation', async () => {
        console.log(`🔍 Testing daemon TypeScript compilation...`);
        
        // Run TypeScript compiler on daemon modules
        const { spawn } = await import('child_process');
        
        return new Promise((resolve, reject) => {
          const tscProcess = spawn('npx', ['tsc', '--noEmit', '--project', '.'], {
            cwd: this.rootDir,
            stdio: 'pipe'
          });
          
          let stdout = '';
          let stderr = '';
          
          tscProcess.stdout?.on('data', (data) => {
            stdout += data.toString();
          });
          
          tscProcess.stderr?.on('data', (data) => {
            stderr += data.toString();
          });
          
          tscProcess.on('close', (code) => {
            if (code === 0) {
              console.log(`✅ TypeScript compilation successful`);
              resolve(true);
            } else {
              console.log(`❌ TypeScript compilation failed with code ${code}`);
              if (stderr) console.log(`Error output:`, stderr);
              resolve(false); // Don't reject, just mark as failed
            }
          });
          
          // Timeout after 30 seconds
          setTimeout(() => {
            tscProcess.kill();
            reject(new Error('TypeScript compilation timed out'));
          }, 30000);
        });
      });

    } catch (error) {
      console.error(`❌ Daemon testing failed:`, error);
      throw error;
    }
  }

  private async runTest(category: string, testName: string, testFn: () => Promise<boolean>): Promise<void> {
    const startTime = Date.now();
    
    try {
      console.log(`🧪 Running: ${testName}...`);
      const passed = await testFn();
      const duration = Date.now() - startTime;
      
      this.results.push({
        category,
        testName,
        passed,
        duration
      });
      
      if (passed) {
        console.log(`✅ ${testName} passed (${duration}ms)`);
      } else {
        console.log(`❌ ${testName} failed (${duration}ms)`);
      }
      
    } catch (error) {
      const duration = Date.now() - startTime;
      
      this.results.push({
        category,
        testName,
        passed: false,
        error: error instanceof Error ? error.message : String(error),
        duration
      });
      
      console.log(`❌ ${testName} failed (${duration}ms): ${error}`);
      throw error;
    }
  }

  private printResults(): void {
    console.log(`\n📊 TEST RESULTS SUMMARY`);
    console.log(`════════════════════════`);
    
    const passed = this.results.filter(r => r.passed).length;
    const failed = this.results.filter(r => !r.passed).length;
    
    console.log(`✅ Passed: ${passed}`);
    console.log(`❌ Failed: ${failed}`);
    console.log(`📊 Total:  ${this.results.length}`);
    
    if (failed > 0) {
      console.log(`\n❌ FAILED TESTS:`);
      this.results.filter(r => !r.passed).forEach(result => {
        console.log(`   ${result.category}/${result.testName}: ${result.error || 'Test failed'}`);
      });
    }
  }
}

// CLI Entry Point
async function main() {
  const args = process.argv.slice(2);
  
  if (args.length === 0) {
    console.error(`Usage: npx tsx src/testing/ModularTestRunner.ts <category>`);
    console.error(`Categories: widget, daemon`);
    process.exit(1);
  }
  
  const category = args[0];
  const runner = new ModularTestRunner();
  
  await runner.runTests(category);
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch(error => {
    console.error(`❌ Test runner crashed:`, error);
    process.exit(1);
  });
}

export { ModularTestRunner };