#!/usr/bin/env tsx
/**
 * Simple Widget Test Runner - Direct test verification
 * Bypasses complex discovery system for immediate results
 */

import * as path from 'path';
import * as fs from 'fs';

interface TestFileStatus {
  widget: string;
  hasUnitTests: boolean;
  hasIntegrationTests: boolean;
  missingTests: string[];
}

class SimpleWidgetTestRunner {
  private rootDir: string;

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

  async runSimpleWidgetTest(): Promise<void> {
    console.log(`🧪 SIMPLE WIDGET TEST RUNNER`);
    console.log(`═══════════════════════════`);
    
    const componentsDir = path.join(this.rootDir, 'src', 'ui', 'components');
    
    if (!fs.existsSync(componentsDir)) {
      console.error(`❌ Components directory not found: ${componentsDir}`);
      return;
    }
    
    const widgets = this.discoverWidgets(componentsDir);
    console.log(`📦 Found ${widgets.length} widgets`);
    
    let totalWidgets = 0;
    let widgetsWithTests = 0;
    let totalTestFiles = 0;
    
    for (const widget of widgets) {
      totalWidgets++;
      
      const status = this.checkWidgetTests(widget.path);
      
      if (status.hasUnitTests || status.hasIntegrationTests) {
        widgetsWithTests++;
      }
      
      if (status.hasUnitTests) totalTestFiles++;
      if (status.hasIntegrationTests) totalTestFiles++;
      
      const testStatus = status.hasUnitTests && status.hasIntegrationTests ? '✅ COMPLETE' :
                        status.hasUnitTests || status.hasIntegrationTests ? '🟡 PARTIAL' :
                        '❌ MISSING';
      
      console.log(`  ${testStatus} ${widget.name}`);
      
      if (status.missingTests.length > 0) {
        console.log(`    Missing: ${status.missingTests.join(', ')}`);
      }
    }
    
    console.log(`\n📊 SUMMARY:`);
    console.log(`  Total Widgets: ${totalWidgets}`);
    console.log(`  Widgets with Tests: ${widgetsWithTests}`);
    console.log(`  Total Test Files: ${totalTestFiles}`);
    console.log(`  Coverage: ${((widgetsWithTests / totalWidgets) * 100).toFixed(1)}%`);
    
    if (widgetsWithTests === totalWidgets) {
      console.log(`\n✅ ALL WIDGETS HAVE TESTS! 🎉`);
    } else {
      console.log(`\n🎯 Need tests for ${totalWidgets - widgetsWithTests} more widgets`);
    }
  }

  private discoverWidgets(componentsDir: string): { name: string; path: string }[] {
    const widgets: { name: string; path: string }[] = [];
    
    try {
      const entries = fs.readdirSync(componentsDir, { withFileTypes: true });
      
      for (const entry of entries) {
        if (entry.isDirectory() && entry.name !== 'test' && entry.name !== 'shared') {
          const widgetPath = path.join(componentsDir, entry.name);
          
          // Check if it has TypeScript files (likely a widget)
          const files = fs.readdirSync(widgetPath);
          const hasTypeScript = files.some(file => file.endsWith('.ts'));
          
          if (hasTypeScript) {
            widgets.push({
              name: entry.name,
              path: widgetPath
            });
          }
        }
      }
    } catch (error) {
      console.error('Error discovering widgets:', error);
    }
    
    return widgets;
  }

  private checkWidgetTests(widgetPath: string): TestFileStatus {
    const widgetName = path.basename(widgetPath);
    const testDir = path.join(widgetPath, 'test');
    
    const status: TestFileStatus = {
      widget: widgetName,
      hasUnitTests: false,
      hasIntegrationTests: false,
      missingTests: []
    };
    
    if (!fs.existsSync(testDir)) {
      status.missingTests.push('test directory', 'unit tests', 'integration tests');
      return status;
    }
    
    // Check for unit tests
    const unitTestDir = path.join(testDir, 'unit');
    if (fs.existsSync(unitTestDir)) {
      const unitFiles = fs.readdirSync(unitTestDir);
      status.hasUnitTests = unitFiles.some(file => file.endsWith('.test.ts'));
    }
    
    if (!status.hasUnitTests) {
      status.missingTests.push('unit tests');
    }
    
    // Check for integration tests
    const integrationTestDir = path.join(testDir, 'integration');
    if (fs.existsSync(integrationTestDir)) {
      const integrationFiles = fs.readdirSync(integrationTestDir);
      status.hasIntegrationTests = integrationFiles.some(file => file.endsWith('.test.ts'));
    }
    
    if (!status.hasIntegrationTests) {
      status.missingTests.push('integration tests');
    }
    
    return status;
  }
}

// CLI Entry Point
async function main() {
  const runner = new SimpleWidgetTestRunner();
  await runner.runSimpleWidgetTest();
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch(error => {
    console.error(`❌ Test runner crashed:`, error);
    process.exit(1);
  });
}

export { SimpleWidgetTestRunner };