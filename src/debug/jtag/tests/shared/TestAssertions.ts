/**
 * Test Assertions - Comprehensive assertion utilities for test validation
 * 
 * Provides standardized assertion patterns with descriptive error messages
 * and proper type checking across all test suites.
 */

import { TestExecutionResult, TestSuiteResult } from './TestExecution';
import { ScreenshotResult } from './ScreenshotTesting';
import { ThemeTestResult, ThemeSuiteResult } from './ThemeTesting';
import { FILE_VALIDATION } from './TestConstants';

// =============================================================================
// ASSERTION INTERFACES - Type-safe assertion patterns
// =============================================================================

export interface AssertionOptions {
  context?: string;
  showDetails?: boolean;
  throwOnFailure?: boolean;
}

export interface AssertionResult {
  success: boolean;
  message: string;
  details?: string;
}

// =============================================================================
// CORE ASSERTION UTILITIES - Basic assertion patterns
// =============================================================================

export class TestAssertions {
  
  /**
   * Assert test execution result is successful
   */
  static assertTestSuccess(
    result: TestExecutionResult, 
    options: AssertionOptions = {}
  ): AssertionResult {
    const context = options.context || 'Test';
    
    if (result.success) {
      return {
        success: true,
        message: `✅ ${context}: ${result.testName} passed (${result.duration}ms)`
      };
    }
    
    const message = `❌ ${context}: ${result.testName} failed`;
    const details = `Error: ${result.error}, Duration: ${result.duration}ms`;
    
    if (options.throwOnFailure !== false) {
      throw new Error(`${message} - ${details}`);
    }
    
    return {
      success: false,
      message,
      details
    };
  }
  
  /**
   * Assert all tests in suite are successful
   */
  static assertSuiteSuccess(
    suiteResult: TestSuiteResult,
    options: AssertionOptions = {}
  ): AssertionResult {
    const { summary } = suiteResult;
    const context = options.context || 'Test Suite';
    
    if (summary.failed === 0) {
      return {
        success: true,
        message: `✅ ${context}: ${suiteResult.suiteName} - all ${summary.total} tests passed (${summary.successRate}%)`
      };
    }
    
    const message = `❌ ${context}: ${suiteResult.suiteName} - ${summary.failed}/${summary.total} tests failed`;
    const failedTests = suiteResult.results
      .filter(r => !r.success)
      .map(r => `${r.testName}: ${r.error}`)
      .join(', ');
    
    const details = `Failed tests: ${failedTests}`;
    
    if (options.throwOnFailure !== false) {
      throw new Error(`${message}. ${details}`);
    }
    
    return {
      success: false,
      message,
      details
    };
  }
  
  /**
   * Assert screenshot result is valid
   */
  static assertScreenshotSuccess(
    screenshotResult: ScreenshotResult,
    options: AssertionOptions = {}
  ): AssertionResult {
    const context = options.context || 'Screenshot';
    
    if (!screenshotResult.success) {
      const message = `❌ ${context}: Capture failed`;
      const details = `Error: ${screenshotResult.error}, Capture time: ${screenshotResult.captureTime}ms`;
      
      if (options.throwOnFailure !== false) {
        throw new Error(`${message} - ${details}`);
      }
      
      return { success: false, message, details };
    }
    
    if (!screenshotResult.filepath) {
      const message = `❌ ${context}: No file path returned`;
      
      if (options.throwOnFailure !== false) {
        throw new Error(message);
      }
      
      return { success: false, message };
    }
    
    // Validate file size if available
    if (screenshotResult.fileSize && screenshotResult.fileSize < FILE_VALIDATION.MIN_SCREENSHOT_SIZE) {
      const message = `❌ ${context}: File too small (${screenshotResult.fileSize} bytes)`;
      
      if (options.throwOnFailure !== false) {
        throw new Error(message);
      }
      
      return { success: false, message };
    }
    
    return {
      success: true,
      message: `✅ ${context}: Valid (${screenshotResult.fileSize} bytes, ${screenshotResult.captureTime}ms)`,
      details: `File: ${screenshotResult.filepath}, Dimensions: ${screenshotResult.dimensions?.width}x${screenshotResult.dimensions?.height}`
    };
  }
  
  /**
   * Assert theme test result is successful
   */
  static assertThemeTestSuccess(
    themeResult: ThemeTestResult,
    options: AssertionOptions = {}
  ): AssertionResult {
    const context = options.context || 'Theme Test';
    
    if (!themeResult.success) {
      const message = `❌ ${context}: Theme "${themeResult.theme}" failed`;
      const details = `Error: ${themeResult.error}, Duration: ${themeResult.totalTime}ms`;
      
      if (options.throwOnFailure !== false) {
        throw new Error(`${message} - ${details}`);
      }
      
      return { success: false, message, details };
    }
    
    // Validate screenshot if present
    if (themeResult.screenshotResult) {
      try {
        this.assertScreenshotSuccess(themeResult.screenshotResult, {
          context: `${context} Screenshot`,
          throwOnFailure: true
        });
      } catch (error) {
        const message = `❌ ${context}: Theme "${themeResult.theme}" screenshot validation failed`;
        
        if (options.throwOnFailure !== false) {
          throw error;
        }
        
        return {
          success: false,
          message,
          details: error instanceof Error ? error.message : String(error)
        };
      }
    }
    
    return {
      success: true,
      message: `✅ ${context}: Theme "${themeResult.theme}" passed (${themeResult.totalTime}ms)`,
      details: `Switch: ${themeResult.switchTime}ms, Screenshot: ${themeResult.screenshotTime}ms`
    };
  }
  
  /**
   * Assert theme suite results are successful
   */
  static assertThemeSuiteSuccess(
    suiteResult: ThemeSuiteResult,
    options: AssertionOptions = {}
  ): AssertionResult {
    const { summary } = suiteResult;
    const context = options.context || 'Theme Suite';
    
    if (summary.failed === 0 && summary.successful > 0) {
      return {
        success: true,
        message: `✅ ${context}: All ${summary.total} themes passed (${summary.successRate}%)`,
        details: `Duration: ${summary.totalDuration}ms`
      };
    }
    
    if (summary.successful === 0) {
      const message = `❌ ${context}: No themes were successfully tested`;
      
      if (options.throwOnFailure !== false) {
        throw new Error(message);
      }
      
      return { success: false, message };
    }
    
    const message = `❌ ${context}: ${summary.failed}/${summary.total} themes failed`;
    const failedThemes = suiteResult.themes
      .filter(t => !t.success)
      .map(t => `${t.theme}: ${t.error}`)
      .join(', ');
    
    const details = `Failed themes: ${failedThemes}`;
    
    if (options.throwOnFailure !== false) {
      throw new Error(`${message}. ${details}`);
    }
    
    return { success: false, message, details };
  }
  
  /**
   * Assert file exists and meets size requirements
   */
  static assertFileValid(
    filepath: string,
    expectedMinSize?: number,
    options: AssertionOptions = {}
  ): AssertionResult {
    const context = options.context || 'File Validation';
    const minSize = expectedMinSize || FILE_VALIDATION.MIN_SCREENSHOT_SIZE;
    
    try {
      const fs = require('fs');
      const stats = fs.statSync(filepath);
      
      if (stats.size < minSize) {
        const message = `❌ ${context}: File too small (${stats.size} bytes, min: ${minSize})`;
        
        if (options.throwOnFailure !== false) {
          throw new Error(message);
        }
        
        return { success: false, message };
      }
      
      return {
        success: true,
        message: `✅ ${context}: File valid (${stats.size} bytes)`,
        details: `Path: ${filepath}`
      };
      
    } catch (error) {
      const message = `❌ ${context}: File access failed`;
      const details = error instanceof Error ? error.message : String(error);
      
      if (options.throwOnFailure !== false) {
        throw new Error(`${message} - ${details}`);
      }
      
      return { success: false, message, details };
    }
  }
  
  /**
   * Assert value meets expectations with custom validation
   */
  static assertValue<T>(
    actual: T,
    expected: T,
    description: string,
    options: AssertionOptions = {}
  ): AssertionResult {
    const context = options.context || 'Value Assertion';
    
    if (actual === expected) {
      return {
        success: true,
        message: `✅ ${context}: ${description} matches expected value`
      };
    }
    
    const message = `❌ ${context}: ${description} mismatch`;
    const details = `Expected: ${expected}, Actual: ${actual}`;
    
    if (options.throwOnFailure !== false) {
      throw new Error(`${message} - ${details}`);
    }
    
    return { success: false, message, details };
  }
}

// =============================================================================
// CONVENIENCE ASSERTION FUNCTIONS - Simple interfaces
// =============================================================================

/**
 * Quick success assertion with context
 */
export function assertTestPassed(
  result: TestExecutionResult,
  context?: string
): void {
  TestAssertions.assertTestSuccess(result, { 
    context, 
    throwOnFailure: true 
  });
}

/**
 * Quick suite success assertion
 */
export function assertSuitePassed(
  suiteResult: TestSuiteResult,
  context?: string
): void {
  TestAssertions.assertSuiteSuccess(suiteResult, {
    context,
    throwOnFailure: true
  });
}

/**
 * Quick screenshot validation
 */
export function assertScreenshotValid(
  screenshotResult: ScreenshotResult,
  context?: string
): void {
  TestAssertions.assertScreenshotSuccess(screenshotResult, {
    context,
    throwOnFailure: true
  });
}

/**
 * Quick theme test validation
 */
export function assertThemeTestPassed(
  themeResult: ThemeTestResult,
  context?: string
): void {
  TestAssertions.assertThemeTestSuccess(themeResult, {
    context,
    throwOnFailure: true
  });
}

/**
 * Quick file validation
 */
export function assertFileExists(
  filepath: string,
  minSize?: number,
  context?: string
): void {
  TestAssertions.assertFileValid(filepath, minSize, {
    context,
    throwOnFailure: true
  });
}

// Export main assertions class
export { TestAssertions };