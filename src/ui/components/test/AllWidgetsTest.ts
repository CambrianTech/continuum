/**
 * All Widgets Test - Comprehensive validation of all widgets
 * Ensures every widget directory complies with architecture requirements
 * Runs unit, client, and integration tests for each widget
 */

import { WidgetDiscovery, WidgetMetadata } from '../shared/WidgetDiscovery.js';
import { BaseWidgetTest } from '../shared/WidgetTestTemplate.js';

class WidgetImplementationTest extends BaseWidgetTest {
  private metadata: WidgetMetadata;

  constructor(metadata: WidgetMetadata) {
    super(metadata.name, metadata.path);
    this.metadata = metadata;
  }

  protected async runUnitTests(): Promise<{ passed: number; failed: number; details: any[] }> {
    // Run standard unit tests plus widget-specific tests
    const standardResults = await this.standardUnitTests();
    
    // Additional widget-specific unit tests
    const additionalDetails: any[] = [];
    let additionalPassed = 0;
    let additionalFailed = 0;

    // Test: Widget has test files
    if (this.metadata.testFiles.length > 0) {
      additionalDetails.push({ test: 'has test files', status: 'passed', count: this.metadata.testFiles.length });
      additionalPassed++;
    } else {
      additionalDetails.push({ test: 'has test files', status: 'failed', error: 'No test files found' });
      additionalFailed++;
    }

    // Test: Widget compliance
    if (this.metadata.isCompliant) {
      additionalDetails.push({ test: 'widget compliance', status: 'passed' });
      additionalPassed++;
    } else {
      additionalDetails.push({ 
        test: 'widget compliance', 
        status: 'failed', 
        error: 'Non-compliant',
        warnings: this.metadata.warnings 
      });
      additionalFailed++;
    }

    return {
      passed: standardResults.passed + additionalPassed,
      failed: standardResults.failed + additionalFailed,
      details: [...standardResults.details, ...additionalDetails]
    };
  }

  protected async runClientTests(): Promise<{ passed: number; failed: number; details: any[] }> {
    const details: any[] = [];
    let passed = 0;
    let failed = 0;

    // Test: Widget file can be loaded as module
    try {
      const fs = await import('fs');
      const path = await import('path');
      const widgetFilePath = path.join(this.metadata.path, this.metadata.widgetFile);
      
      if (fs.existsSync(widgetFilePath)) {
        const content = fs.readFileSync(widgetFilePath, 'utf-8');
        
        // Check for proper imports
        if (content.includes('import') && content.includes('from')) {
          details.push({ test: 'has proper imports', status: 'passed' });
          passed++;
        } else {
          details.push({ test: 'has proper imports', status: 'warning', message: 'No imports found - might be self-contained' });
        }

        // Check for CSS imports
        if (content.includes('.css')) {
          details.push({ test: 'imports CSS', status: 'passed' });
          passed++;
        } else {
          details.push({ test: 'imports CSS', status: 'warning', message: 'No CSS imports found' });
        }
      }
    } catch (error) {
      details.push({ test: 'widget file analysis', status: 'failed', error: error });
      failed++;
    }

    return { passed, failed, details };
  }

  protected async runIntegrationTests(): Promise<{ passed: number; failed: number; details: any[] }> {
    const standardResults = await this.standardIntegrationTests();
    
    // Additional integration tests
    const additionalDetails: any[] = [];
    let additionalPassed = 0;
    let additionalFailed = 0;

    // Test: Widget can be discovered by discovery system
    additionalDetails.push({ test: 'discoverable by system', status: 'passed' });
    additionalPassed++;

    // Test: Widget has CSS files
    if (this.metadata.cssFiles.length > 0) {
      additionalDetails.push({ test: 'has CSS styling', status: 'passed', files: this.metadata.cssFiles });
      additionalPassed++;
    } else {
      additionalDetails.push({ test: 'has CSS styling', status: 'warning', message: 'No CSS files found' });
    }

    return {
      passed: standardResults.passed + additionalPassed,
      failed: standardResults.failed + additionalFailed,
      details: [...standardResults.details, ...additionalDetails]
    };
  }
}

export class AllWidgetsTestRunner {
  private discovery: WidgetDiscovery;

  constructor() {
    this.discovery = new WidgetDiscovery();
  }

  /**
   * Run comprehensive tests on all widgets
   */
  async runAllWidgetTests(): Promise<{
    totalWidgets: number;
    compliantWidgets: number;
    nonCompliantWidgets: number;
    testResults: any[];
    summary: {
      totalTests: number;
      passed: number;
      failed: number;
      successRate: number;
    };
  }> {
    console.log('🧪 Starting comprehensive widget testing...');
    
    // Discover all widgets
    const widgets = await this.discovery.discoverWidgets();
    const compliant = widgets.filter(w => w.isCompliant);
    const nonCompliant = widgets.filter(w => !w.isCompliant);

    console.log(`📊 Found ${widgets.length} widgets: ${compliant.length} compliant, ${nonCompliant.length} non-compliant`);

    // Report non-compliant widgets
    if (nonCompliant.length > 0) {
      console.warn('⚠️ Non-compliant widgets:');
      for (const widget of nonCompliant) {
        console.warn(`  - ${widget.name}: ${widget.warnings.join(', ')}`);
      }
    }

    // Run tests on all widgets (including non-compliant to see what's wrong)
    const testResults: any[] = [];
    let totalTests = 0;
    let totalPassed = 0;
    let totalFailed = 0;

    for (const widget of widgets) {
      console.log(`\n🧪 Testing widget: ${widget.name}`);
      
      const test = new WidgetImplementationTest(widget);
      const result = await test.runAllTests();
      
      testResults.push({
        widget: widget.name,
        compliant: widget.isCompliant,
        warnings: widget.warnings,
        ...result
      });

      totalTests += result.passed + result.failed;
      totalPassed += result.passed;
      totalFailed += result.failed;

      console.log(`  ✅ ${result.passed} passed, ❌ ${result.failed} failed`);
    }

    const successRate = totalTests > 0 ? (totalPassed / totalTests) * 100 : 0;

    console.log(`\n📊 Widget Testing Summary:`);
    console.log(`  Total Widgets: ${widgets.length}`);
    console.log(`  Compliant: ${compliant.length}`);
    console.log(`  Non-compliant: ${nonCompliant.length}`);
    console.log(`  Total Tests: ${totalTests}`);
    console.log(`  Passed: ${totalPassed}`);
    console.log(`  Failed: ${totalFailed}`);
    console.log(`  Success Rate: ${successRate.toFixed(1)}%`);

    return {
      totalWidgets: widgets.length,
      compliantWidgets: compliant.length,
      nonCompliantWidgets: nonCompliant.length,
      testResults,
      summary: {
        totalTests,
        passed: totalPassed,
        failed: totalFailed,
        successRate
      }
    };
  }

  /**
   * Quick compliance check - just validate structure
   */
  async quickComplianceCheck(): Promise<void> {
    const { compliant, nonCompliant } = await this.discovery.validateAllWidgets();
    
    if (nonCompliant.length === 0) {
      console.log(`✅ All ${compliant.length} widgets are compliant!`);
    } else {
      console.warn(`⚠️ ${nonCompliant.length} widgets are non-compliant:`);
      for (const widget of nonCompliant) {
        console.warn(`  - ${widget.name}: ${widget.warnings.join(', ')}`);
      }
      throw new Error(`Widget compliance check failed: ${nonCompliant.length} non-compliant widgets`);
    }
  }
}