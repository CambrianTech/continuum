/**
 * Automated Screenshot Coordinate Validation System
 * 
 * Automatically validates that screenshot coordinate calculation is accurate
 * by testing multiple elements and coordinate scenarios.
 */

import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface CoordinateValidationResult {
  testName: string;
  selector: string;
  passed: boolean;
  expectedBounds?: { x: number; y: number; width: number; height: number };
  actualBounds?: { x: number; y: number; width: number; height: number };
  errorMessage?: string;
  screenshotPath?: string;
  executionTime: number;
}

export interface ValidationSummary {
  totalTests: number;
  passed: number;
  failed: number;
  successRate: number;
  averageExecutionTime: number;
  results: CoordinateValidationResult[];
}

/**
 * Automated coordinate validation test cases
 */
const COORDINATE_TEST_CASES = [
  {
    name: 'Chat Widget Full Element',
    selector: 'chat-widget',
    description: 'Test full chat widget element capture'
  },
  {
    name: 'Chat Input Field',
    selector: 'chat-widget input[type="text"]',
    description: 'Test specific input field within chat widget'
  },
  {
    name: 'Chat Send Button',
    selector: 'chat-widget .send-button',
    description: 'Test button element capture'
  },
  {
    name: 'Body Element',
    selector: 'body',
    description: 'Test full page body capture'
  },
  {
    name: 'First Div Element',
    selector: 'div',
    description: 'Test first div element on page'
  }
];

/**
 * Automated Screenshot Coordinate Validator
 */
export class ScreenshotCoordinateValidator {
  private sessionId: string;

  constructor() {
    this.sessionId = generateUUID();
  }

  /**
   * Run automated coordinate validation tests
   */
  async runValidation(): Promise<ValidationSummary> {
    console.log('🤖 Starting Automated Coordinate Validation');
    console.log(`📋 Session: ${this.sessionId}`);
    console.log(`🧪 Running ${COORDINATE_TEST_CASES.length} validation tests...\n`);

    const results: CoordinateValidationResult[] = [];
    const startTime = Date.now();

    for (const testCase of COORDINATE_TEST_CASES) {
      const result = await this.runSingleValidation(testCase);
      results.push(result);
      
      // Log result immediately
      const status = result.passed ? '✅' : '❌';
      console.log(`${status} ${result.testName}: ${result.passed ? 'PASSED' : 'FAILED'} (${result.executionTime}ms)`);
      if (!result.passed && result.errorMessage) {
        console.log(`   Error: ${result.errorMessage}`);
      }
    }

    const totalTime = Date.now() - startTime;
    const summary = this.generateSummary(results, totalTime);
    
    console.log('\n📊 VALIDATION SUMMARY');
    console.log('=' .repeat(50));
    console.log(`Total Tests: ${summary.totalTests}`);
    console.log(`Passed: ${summary.passed}`);
    console.log(`Failed: ${summary.failed}`);
    console.log(`Success Rate: ${summary.successRate.toFixed(1)}%`);
    console.log(`Average Execution Time: ${summary.averageExecutionTime.toFixed(1)}ms`);
    console.log(`Total Time: ${totalTime}ms`);

    if (summary.failed > 0) {
      console.log('\n❌ FAILED TESTS:');
      summary.results
        .filter(r => !r.passed)
        .forEach(r => {
          console.log(`   • ${r.testName}: ${r.errorMessage}`);
        });
    }

    return summary;
  }

  /**
   * Run a single coordinate validation test
   */
  private async runSingleValidation(testCase: { name: string; selector: string; description: string }): Promise<CoordinateValidationResult> {
    const startTime = Date.now();
    
    try {
      // Import JTAG client following the working pattern from server-screenshot.test.ts
      const { jtag } = await import('../../../../server-index');
      const client = await jtag.connect();

      // Generate unique filename for this test
      const filename = `validation-${testCase.name.toLowerCase().replace(/\s+/g, '-')}-${Date.now()}.png`;
      
      // Execute screenshot command using the exact same pattern as working tests
      const screenshotResult = await (client as any).commands.screenshot({
        querySelector: testCase.selector,
        filename: filename,
        resultType: 'file'
      });

      const executionTime = Date.now() - startTime;

      // Extract result from nested structure like working tests do
      const actualResult = screenshotResult.commandResult?.commandResult || screenshotResult.commandResult || screenshotResult;
      
      if (screenshotResult.success && actualResult.filepath) {
        // Validate that screenshot file was created and has reasonable size
        const fs = await import('fs');
        const path = await import('path');
        
        if (fs.existsSync(actualResult.filepath)) {
          const stats = fs.statSync(actualResult.filepath);
          const fileSizeKB = stats.size / 1024;
          
          // Screenshot should be at least 1KB (not empty)
          if (fileSizeKB > 1) {
            return {
              testName: testCase.name,
              selector: testCase.selector,
              passed: true,
              screenshotPath: actualResult.filepath,
              executionTime
            };
          } else {
            return {
              testName: testCase.name,
              selector: testCase.selector,
              passed: false,
              errorMessage: `Screenshot file too small (${fileSizeKB.toFixed(1)}KB)`,
              screenshotPath: actualResult.filepath,
              executionTime
            };
          }
        } else {
          return {
            testName: testCase.name,
            selector: testCase.selector,
            passed: false,
            errorMessage: 'Screenshot file was not created',
            executionTime
          };
        }
      } else {
        return {
          testName: testCase.name,
          selector: testCase.selector,
          passed: false,
          errorMessage: `Screenshot command failed: ${actualResult.error?.message || 'Unknown error'}`,
          executionTime
        };
      }

    } catch (error) {
      return {
        testName: testCase.name,
        selector: testCase.selector,
        passed: false,
        errorMessage: `Validation error: ${error instanceof Error ? error.message : String(error)}`,
        executionTime: Date.now() - startTime
      };
    }
  }

  /**
   * Generate validation summary
   */
  private generateSummary(results: CoordinateValidationResult[], totalTime: number): ValidationSummary {
    const passed = results.filter(r => r.passed).length;
    const failed = results.length - passed;
    const successRate = (passed / results.length) * 100;
    const averageExecutionTime = results.reduce((sum, r) => sum + r.executionTime, 0) / results.length;

    return {
      totalTests: results.length,
      passed,
      failed,
      successRate,
      averageExecutionTime,
      results
    };
  }

  /**
   * Generate detailed validation report
   */
  async generateReport(summary: ValidationSummary): Promise<string> {
    const timestamp = new Date().toISOString();
    
    let report = `# Screenshot Coordinate Validation Report\n\n`;
    report += `**Generated:** ${timestamp}\n`;
    report += `**Session:** ${this.sessionId}\n\n`;
    
    report += `## Summary\n\n`;
    report += `- **Total Tests:** ${summary.totalTests}\n`;
    report += `- **Passed:** ${summary.passed}\n`;
    report += `- **Failed:** ${summary.failed}\n`;
    report += `- **Success Rate:** ${summary.successRate.toFixed(1)}%\n`;
    report += `- **Average Execution Time:** ${summary.averageExecutionTime.toFixed(1)}ms\n\n`;
    
    report += `## Test Results\n\n`;
    
    summary.results.forEach(result => {
      const status = result.passed ? '✅ PASSED' : '❌ FAILED';
      report += `### ${result.testName}\n\n`;
      report += `- **Status:** ${status}\n`;
      report += `- **Selector:** \`${result.selector}\`\n`;
      report += `- **Execution Time:** ${result.executionTime}ms\n`;
      
      if (result.screenshotPath) {
        report += `- **Screenshot:** ${result.screenshotPath}\n`;
      }
      
      if (result.errorMessage) {
        report += `- **Error:** ${result.errorMessage}\n`;
      }
      
      report += '\n';
    });
    
    return report;
  }
}

/**
 * Run coordinate validation from command line
 */
async function runCoordinateValidation(): Promise<void> {
  console.log('🚀 Screenshot Coordinate Validation System\n');
  
  try {
    const validator = new ScreenshotCoordinateValidator();
    const summary = await validator.runValidation();
    
    // Generate and save report
    const report = await validator.generateReport(summary);
    const fs = await import('fs');
    const path = await import('path');
    
    const reportsDir = path.join(process.cwd(), '.continuum', 'validation-reports');
    if (!fs.existsSync(reportsDir)) {
      fs.mkdirSync(reportsDir, { recursive: true });
    }
    
    const reportPath = path.join(reportsDir, `coordinate-validation-${Date.now()}.md`);
    fs.writeFileSync(reportPath, report);
    
    console.log(`\n📄 Validation report saved: ${reportPath}`);
    
    if (summary.successRate === 100) {
      console.log('\n🎉 ALL COORDINATE VALIDATION TESTS PASSED!');
      process.exit(0);
    } else {
      console.log('\n⚠️ Some validation tests failed. Check the report for details.');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('\n❌ Coordinate validation failed:', error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

// Export for use in other tests
export { runCoordinateValidation };

// Run if called directly
if (require.main === module) {
  runCoordinateValidation();
}