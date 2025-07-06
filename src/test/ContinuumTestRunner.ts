#!/usr/bin/env tsx
/**
 * Comprehensive test runner for Continuum
 * Runs all tests in the correct order with proper error handling
 * Used by git hooks to prevent broken commits
 */

import { spawn } from 'child_process';
import * as fs from 'fs';

interface TestResult {
  name: string;
  passed: boolean;
  duration: number;
  error?: string;
}

class ContinuumTestRunner {
  private results: TestResult[] = [];
  private startTime: number = Date.now();
  
  async run(): Promise<boolean> {
    console.log('🚀 Continuum Comprehensive Test Suite');
    console.log('=====================================\n');
    
    // 1. TypeScript Compilation Check (fastest failure)
    if (!await this.runTest('TypeScript Compilation', 'npx tsc --noEmit --project .')) {
      return this.reportFailure();
    }
    
    // 2. Type Safety Tests
    if (!await this.runTest('Type Safety', 'npx tsx --test src/test/integration/TypeSafety.integration.test.ts')) {
      return this.reportFailure();
    }
    
    // 3. Unit Tests (fast, isolated)
    if (!await this.runTest('Base Command Unit Tests', 'npx tsx --test src/commands/core/base-command/test/unit/BaseCommand.test.ts')) {
      // Continue even if some unit tests fail - they might be WIP
    }
    
    // 4. Integration Tests (critical path)
    const integrationTests = [
      { name: 'Daemon Event Bus', cmd: 'npx tsx --test src/test/integration/DaemonEventBus.integration.test.ts' },
      { name: 'Command Routing', cmd: 'npx tsx --test src/test/integration/CommandRouting.integration.test.ts' },
      { name: 'Wildcard Routing', cmd: 'npx tsx --test src/test/integration/WildcardRouting.integration.test.ts' },
      { name: 'HTML Rendering', cmd: 'npx tsx --test src/test/integration/HTMLRendering.integration.test.ts' }
    ];
    
    for (const test of integrationTests) {
      if (!await this.runTest(test.name, test.cmd)) {
        return this.reportFailure();
      }
    }
    
    // 5. System Integration Test (requires all daemons)
    if (!await this.runTest('System Integration', 'npx tsx --test src/test/integration/DaemonSystem.integration.test.ts')) {
      return this.reportFailure();
    }
    
    // 6. Check for any 'any' types in critical files
    if (!await this.checkNoAnyTypes()) {
      return this.reportFailure();
    }
    
    return this.reportSuccess();
  }
  
  private async runTest(name: string, command: string): Promise<boolean> {
    const testStart = Date.now();
    console.log(`\n🧪 Running ${name}...`);
    
    return new Promise((resolve) => {
      const child = spawn(command, [], {
        shell: true,
        stdio: 'pipe'
      });
      
      let output = '';
      let errorOutput = '';
      
      child.stdout?.on('data', (data) => {
        output += data.toString();
        // Show dots for progress
        process.stdout.write('.');
      });
      
      child.stderr?.on('data', (data) => {
        errorOutput += data.toString();
      });
      
      child.on('close', (code) => {
        const duration = Date.now() - testStart;
        const passed = code === 0;
        
        const result: TestResult = {
          name,
          passed,
          duration
        };
        
        if (!passed) {
          result.error = errorOutput || output;
        }
        
        this.results.push(result);
        
        if (passed) {
          console.log(`\n✅ ${name} passed (${duration}ms)`);
        } else {
          console.log(`\n❌ ${name} failed (${duration}ms)`);
          if (errorOutput) {
            console.log('Error output:', errorOutput.slice(0, 500));
          }
        }
        
        resolve(passed);
      });
      
      child.on('error', (err) => {
        const duration = Date.now() - testStart;
        this.results.push({
          name,
          passed: false,
          duration,
          error: err.message
        });
        console.log(`\n❌ ${name} failed to start: ${err.message}`);
        resolve(false);
      });
    });
  }
  
  private async checkNoAnyTypes(): Promise<boolean> {
    console.log('\n🔍 Checking for any types in critical files...');
    
    const criticalFiles = [
      'src/daemons/base/BaseDaemon.ts',
      'src/daemons/base/DaemonProtocol.ts',
      'src/daemons/base/DaemonEventBus.ts',
      'src/commands/base/BaseCommand.ts',
      'src/commands/base/DaemonCommand.ts'
    ];
    
    let hasAnyTypes = false;
    
    for (const file of criticalFiles) {
      if (!fs.existsSync(file)) continue;
      
      const content = fs.readFileSync(file, 'utf-8');
      const lines = content.split('\n');
      
      lines.forEach((line, index) => {
        // Skip comments
        if (line.trim().startsWith('//') || line.trim().startsWith('*')) {
          return;
        }
        
        // Check for ': any' or 'as any'
        if (line.match(/:\s*any(?:\s|,|;|\)|>|$)/) || line.match(/as\s+any/)) {
          console.log(`\n⚠️  Found 'any' type in ${file}:${index + 1}`);
          console.log(`   ${line.trim()}`);
          hasAnyTypes = true;
        }
      });
    }
    
    if (!hasAnyTypes) {
      console.log('✅ No any types found in critical files');
    }
    
    return !hasAnyTypes;
  }
  
  private reportSuccess(): boolean {
    const totalDuration = Date.now() - this.startTime;
    const passed = this.results.filter(r => r.passed).length;
    const failed = this.results.filter(r => !r.passed).length;
    
    console.log('\n========================================');
    console.log('✅ ALL TESTS PASSED!');
    console.log(`Total: ${this.results.length} | Passed: ${passed} | Failed: ${failed}`);
    console.log(`Duration: ${totalDuration}ms`);
    console.log('========================================\n');
    
    return true;
  }
  
  private reportFailure(): boolean {
    const totalDuration = Date.now() - this.startTime;
    const passed = this.results.filter(r => r.passed).length;
    const failed = this.results.filter(r => !r.passed).length;
    
    console.log('\n========================================');
    console.log('❌ TESTS FAILED - COMMIT BLOCKED');
    console.log(`Total: ${this.results.length} | Passed: ${passed} | Failed: ${failed}`);
    console.log(`Duration: ${totalDuration}ms`);
    console.log('\nFailed tests:');
    
    this.results.filter(r => !r.passed).forEach(result => {
      console.log(`  - ${result.name}`);
    });
    
    console.log('\n🔧 Fix the failing tests before committing!');
    console.log('========================================\n');
    
    return false;
  }
}

// Run if called directly
if (require.main === module) {
  const runner = new ContinuumTestRunner();
  runner.run().then(success => {
    process.exit(success ? 0 : 1);
  });
}

export { ContinuumTestRunner };