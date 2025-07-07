#!/usr/bin/env tsx
/**
 * Simple Test Runner - Minimal test runner without complex imports
 * 
 * Directly tests core functionality without dependency issues
 */

import * as fs from 'fs';
import * as path from 'path';

interface SimpleTestResult {
  name: string;
  passed: boolean;
  details?: string;
}

class SimpleTestRunner {
  private results: SimpleTestResult[] = [];

  async runWidgetTests(): Promise<void> {
    console.log('🧪 Simple Widget Test Runner');
    console.log('============================');

    await this.testWidgetDiscovery();
    await this.testWidgetCompliance();
    await this.testCompilation();

    this.printResults();
  }

  private async testWidgetDiscovery(): Promise<void> {
    try {
      const widgetDir = path.join(process.cwd(), 'src/ui/components');
      const entries = fs.readdirSync(widgetDir, { withFileTypes: true });
      const widgetFolders = entries.filter(entry => entry.isDirectory()).map(entry => entry.name);
      
      this.results.push({
        name: 'Widget Directory Discovery',
        passed: widgetFolders.length > 0,
        details: `Found ${widgetFolders.length} widget directories: ${widgetFolders.join(', ')}`
      });
    } catch (error) {
      this.results.push({
        name: 'Widget Directory Discovery',
        passed: false,
        details: `Error: ${error}`
      });
    }
  }

  private async testWidgetCompliance(): Promise<void> {
    try {
      const widgetDir = path.join(process.cwd(), 'src/ui/components');
      const entries = fs.readdirSync(widgetDir, { withFileTypes: true });
      const widgetFolders = entries.filter(entry => entry.isDirectory()).map(entry => entry.name);
      
      let compliantCount = 0;
      let totalCount = 0;

      for (const folder of widgetFolders) {
        if (folder === 'test' || folder === 'shared') continue; // Skip utility folders
        
        totalCount++;
        const folderPath = path.join(widgetDir, folder);
        
        // Check for package.json
        const packageJsonPath = path.join(folderPath, 'package.json');
        const hasPackageJson = fs.existsSync(packageJsonPath);
        
        // Check for widget implementation file
        const widgetTsPath = path.join(folderPath, `${folder}Widget.ts`);
        const hasWidgetFile = fs.existsSync(widgetTsPath);
        
        if (hasPackageJson && hasWidgetFile) {
          compliantCount++;
        }
      }

      this.results.push({
        name: 'Widget Compliance Check',
        passed: compliantCount > totalCount * 0.7, // 70% compliance threshold
        details: `${compliantCount}/${totalCount} widgets compliant (${Math.round(compliantCount/totalCount*100)}%)`
      });
    } catch (error) {
      this.results.push({
        name: 'Widget Compliance Check',
        passed: false,
        details: `Error: ${error}`
      });
    }
  }

  private async testCompilation(): Promise<void> {
    try {
      // Test if TypeScript compiles without errors
      const { execSync } = await import('child_process');
      execSync('npx tsc --noEmit --project .', { 
        encoding: 'utf8',
        cwd: process.cwd(),
        timeout: 30000
      });
      
      this.results.push({
        name: 'TypeScript Compilation',
        passed: true,
        details: 'No compilation errors'
      });
    } catch (error) {
      this.results.push({
        name: 'TypeScript Compilation',
        passed: false,
        details: `Compilation errors detected`
      });
    }
  }

  private printResults(): void {
    console.log('\n📊 Test Results:');
    console.log('================');

    let passed = 0;
    let total = this.results.length;

    for (const result of this.results) {
      const icon = result.passed ? '✅' : '❌';
      console.log(`${icon} ${result.name}`);
      if (result.details) {
        console.log(`   ${result.details}`);
      }
      if (result.passed) passed++;
    }

    console.log(`\n📈 Summary: ${passed}/${total} tests passed (${Math.round(passed/total*100)}%)`);
    
    if (passed === total) {
      console.log('🎉 All tests passed!');
    } else {
      console.log('⚠️ Some tests failed - check details above');
    }
  }
}

// Main execution
if (import.meta.url === `file://${process.argv[1]}`) {
  const runner = new SimpleTestRunner();
  runner.runWidgetTests().catch(error => {
    console.error('❌ Test runner failed:', error);
    process.exit(1);
  });
}

export { SimpleTestRunner };