/**
 * Modern Test Utilities - Next generation test patterns with zero duplication
 * 
 * Eliminates all duplicate patterns across tests:
 * - Standardized client connections
 * - Common test assertions
 * - Screenshot validation
 * - Error handling
 * - Timing and performance measurement
 * - File validation
 */

import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import { JTAGEnvironment } from '../../system/core/types/JTAGTypes';
import { 
  JTAGClientFactory,
  connectJTAGClient,
  executeJTAGCommand,
  takeJTAGScreenshot,
  ClientConnectionOptions,
  CommandExecutionOptions,
  ClientConnectionResult,
  CommandResult
} from './JTAGClientFactory';

// =============================================================================
// MODERN TEST CONSTANTS - Comprehensive test configuration
// =============================================================================

export const MODERN_TEST_CONSTANTS = {
  TIMEOUTS: {
    QUICK_TEST: 5000,      // 5s for simple operations
    NORMAL_TEST: 15000,    // 15s for standard operations  
    SLOW_TEST: 30000,      // 30s for complex operations
    INTEGRATION_TEST: 60000 // 60s for full integration tests
  },
  
  SELECTORS: {
    BODY: 'body',
    CHAT_WIDGET: 'chat-widget',
    CHAT_INPUT: 'input[placeholder*="message"], textarea[placeholder*="message"], .chat-input, #chat-input',
    CHAT_SEND_BUTTON: 'button[type="submit"], .send-button, .chat-send',
    SIDEBAR: 'continuum-sidebar',
    APP_CONTAINER: '.app-container',
    THEME_SELECTOR: '.theme-selector, [data-theme]'
  },
  
  FILE_VALIDATION: {
    MIN_SCREENSHOT_SIZE: 1000,  // 1KB
    MIN_LOG_SIZE: 100,          // 100 bytes
    MAX_REASONABLE_SCREENSHOT: 10 * 1024 * 1024 // 10MB
  },
  
  RETRY: {
    DEFAULT_ATTEMPTS: 3,
    QUICK_ATTEMPTS: 2,
    PERSISTENT_ATTEMPTS: 5
  },
  
  THEMES: {
    ALL_THEMES: ['base', 'classic', 'cyberpunk', 'light', 'monochrome', 'retro-mac'],
    DEFAULT_THEME: 'base'
  }
} as const;

// =============================================================================
// MODERN TYPESCRIPT INTERFACES - Comprehensive typing
// =============================================================================

export interface ModernTestOptions {
  timeout?: number;
  retryAttempts?: number;
  validateResults?: boolean;
  logProgress?: boolean;
  cleanupAfter?: boolean;
}

export interface TestExecutionResult {
  testName: string;
  success: boolean;
  duration: number;
  error?: string;
  data?: any;
  screenshots?: string[];
  logs?: string[];
}

export interface ScreenshotTestOptions extends ModernTestOptions {
  selector?: string;
  filename?: string;
  validateFile?: boolean;
  includeMetadata?: boolean;
}

export interface ThemeTestResult {
  theme: string;
  success: boolean;
  screenshotPath?: string;
  error?: string;
  switchTime?: number;
  screenshotTime?: number;
}

export interface ChatTestOptions extends ModernTestOptions {
  message?: string;
  waitForResponse?: boolean;
  screenshotBeforeAfter?: boolean;
}

// =============================================================================
// MODERN TEST EXECUTION FRAMEWORK
// =============================================================================

export class ModernTestRunner {
  private clientFactory: JTAGClientFactory;
  private results: TestExecutionResult[] = [];
  private activeClient: JTAGClient | null = null;
  
  constructor() {
    this.clientFactory = JTAGClientFactory.getInstance();
  }
  
  /**
   * Execute test with comprehensive error handling and timing
   */
  async executeTest<T>(
    testName: string,
    testFunction: (client: JTAGClient) => Promise<T>,
    options: ModernTestOptions = {}
  ): Promise<TestExecutionResult> {
    const startTime = Date.now();
    const timeout = options.timeout || MODERN_TEST_CONSTANTS.TIMEOUTS.NORMAL_TEST;
    
    if (options.logProgress !== false) {
      console.log(`🧪 ModernTest: Starting "${testName}" (timeout: ${timeout}ms)`);
    }
    
    try {
      // Reuse client if available, create new if needed
      if (!this.activeClient) {
        const connection = await this.clientFactory.createClient({ timeout });
        this.activeClient = connection.client;
      }
      
      // Execute test function with timeout
      const result = await Promise.race([
        testFunction(this.activeClient),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error(`Test timeout after ${timeout}ms`)), timeout)
        )
      ]);
      
      const duration = Date.now() - startTime;
      
      const testResult: TestExecutionResult = {
        testName,
        success: true,
        duration,
        data: result
      };
      
      this.results.push(testResult);
      
      if (options.logProgress !== false) {
        console.log(`✅ ModernTest: "${testName}" completed (${duration}ms)`);
      }
      
      return testResult;
      
    } catch (error) {
      const duration = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      const testResult: TestExecutionResult = {
        testName,
        success: false,
        duration,
        error: errorMessage
      };
      
      this.results.push(testResult);
      
      console.error(`❌ ModernTest: "${testName}" failed (${duration}ms):`, errorMessage);
      
      return testResult;
      
    } finally {
      // Cleanup if requested
      if (options.cleanupAfter) {
        await this.cleanup();
      }
    }
  }
  
  /**
   * Execute screenshot test with file validation
   */
  async executeScreenshotTest(
    testName: string,
    selector: string = MODERN_TEST_CONSTANTS.SELECTORS.BODY,
    options: ScreenshotTestOptions = {}
  ): Promise<TestExecutionResult> {
    return this.executeTest(testName, async (client) => {
      const filename = options.filename || `${testName.toLowerCase().replace(/\s+/g, '-')}-${Date.now()}`;
      
      console.log(`📸 Taking screenshot of "${selector}" → "${filename}"`);
      
      const result = await this.clientFactory.executeCommand(
        client,
        'screenshot',
        { querySelector: selector, filename },
        { timeout: options.timeout }
      );
      
      if (!result.success) {
        throw new Error(`Screenshot failed: ${result.error}`);
      }
      
      // Validate file if requested
      if (options.validateFile !== false && result.data?.filepath) {
        const fileValid = this.validateScreenshotFile(result.data.filepath);
        if (!fileValid) {
          throw new Error(`Screenshot file validation failed: ${result.data.filepath}`);
        }
      }
      
      return result.data;
    }, options);
  }
  
  /**
   * Execute theme test with screenshot capture
   */
  async executeThemeTest(themeName: string, options: ModernTestOptions = {}): Promise<ThemeTestResult> {
    const testName = `Theme: ${themeName}`;
    
    try {
      if (!this.activeClient) {
        const connection = await this.clientFactory.createClient();
        this.activeClient = connection.client;
      }
      
      console.log(`🎨 Testing theme: ${themeName}`);
      
      // Switch theme with timing
      const switchStartTime = Date.now();
      const switchResult = await this.clientFactory.executeCommand(
        this.activeClient,
        'theme/set',
        { themeName },
        { timeout: options.timeout }
      );
      const switchTime = Date.now() - switchStartTime;
      
      if (!switchResult.success) {
        throw new Error(`Theme switch failed: ${switchResult.error}`);
      }
      
      // Wait for theme to apply
      await this.sleep(1500);
      
      // Take screenshot with timing
      const screenshotStartTime = Date.now();
      const screenshotResult = await this.clientFactory.executeCommand(
        this.activeClient,
        'screenshot',
        { 
          querySelector: 'body',
          filename: `theme-${themeName}.png`
        },
        { timeout: options.timeout }
      );
      const screenshotTime = Date.now() - screenshotStartTime;
      
      if (!screenshotResult.success) {
        throw new Error(`Screenshot failed: ${screenshotResult.error}`);
      }
      
      console.log(`✅ Theme ${themeName} test completed`);
      
      return {
        theme: themeName,
        success: true,
        screenshotPath: screenshotResult.data?.filepath,
        switchTime,
        screenshotTime
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Theme ${themeName} test failed:`, errorMessage);
      
      return {
        theme: themeName,
        success: false,
        error: errorMessage
      };
    }
  }
  
  /**
   * Execute chat interaction test
   */
  async executeChatTest(
    testName: string,
    message: string = 'Test message from ModernTestUtilities',
    options: ChatTestOptions = {}
  ): Promise<TestExecutionResult> {
    return this.executeTest(testName, async (client) => {
      // Find chat input
      const chatInput = await this.findChatInput(client);
      if (!chatInput) {
        throw new Error('Chat input not found');
      }
      
      // Screenshot before if requested
      let beforeScreenshot: string | undefined;
      if (options.screenshotBeforeAfter) {
        const beforeResult = await this.clientFactory.executeCommand(
          client,
          'screenshot',
          { querySelector: 'body', filename: `${testName}-before` }
        );
        beforeScreenshot = beforeResult.data?.filepath;
      }
      
      // Type message
      console.log(`💬 Typing message: "${message}"`);
      const typeResult = await this.clientFactory.executeCommand(
        client,
        'type',
        { selector: chatInput, text: message, clearFirst: true },
        { timeout: options.timeout }
      );
      
      if (!typeResult.success) {
        throw new Error(`Failed to type message: ${typeResult.error}`);
      }
      
      // Screenshot after if requested
      let afterScreenshot: string | undefined;
      if (options.screenshotBeforeAfter) {
        const afterResult = await this.clientFactory.executeCommand(
          client,
          'screenshot',
          { querySelector: 'body', filename: `${testName}-after` }
        );
        afterScreenshot = afterResult.data?.filepath;
      }
      
      return {
        message,
        chatInput,
        screenshots: [beforeScreenshot, afterScreenshot].filter(Boolean)
      };
      
    }, options);
  }
  
  /**
   * Find chat input using common selectors
   */
  private async findChatInput(client: JTAGClient): Promise<string | null> {
    const selectors = MODERN_TEST_CONSTANTS.SELECTORS.CHAT_INPUT.split(', ');
    
    for (const selector of selectors) {
      try {
        const waitResult = await this.clientFactory.executeCommand(
          client,
          'waitForElement',
          { selector, timeout: 2000 },
          { logExecution: false }
        );
        
        if (waitResult.success && waitResult.data?.found) {
          console.log(`✅ Found chat input: ${selector}`);
          return selector;
        }
      } catch (error) {
        // Continue to next selector
      }
    }
    
    return null;
  }
  
  /**
   * Validate screenshot file meets requirements
   */
  private validateScreenshotFile(filepath: string): boolean {
    try {
      const fs = require('fs');
      const stats = fs.statSync(filepath);
      
      const minSize = MODERN_TEST_CONSTANTS.FILE_VALIDATION.MIN_SCREENSHOT_SIZE;
      const maxSize = MODERN_TEST_CONSTANTS.FILE_VALIDATION.MAX_REASONABLE_SCREENSHOT;
      
      if (stats.size < minSize) {
        console.warn(`⚠️ Screenshot file too small: ${stats.size} bytes (min: ${minSize})`);
        return false;
      }
      
      if (stats.size > maxSize) {
        console.warn(`⚠️ Screenshot file too large: ${stats.size} bytes (max: ${maxSize})`);
        return false;
      }
      
      console.log(`✅ Screenshot file valid: ${stats.size} bytes`);
      return true;
      
    } catch (error) {
      console.error(`❌ Failed to validate screenshot file "${filepath}":`, error);
      return false;
    }
  }
  
  /**
   * Get test results summary
   */
  getResults(): TestExecutionResult[] {
    return [...this.results];
  }
  
  /**
   * Get test results summary
   */
  getSummary(): { passed: number; failed: number; total: number; successRate: number } {
    const passed = this.results.filter(r => r.success).length;
    const failed = this.results.filter(r => !r.success).length;
    const total = this.results.length;
    const successRate = total > 0 ? Math.round((passed / total) * 100) : 0;
    
    return { passed, failed, total, successRate };
  }
  
  /**
   * Print comprehensive test results
   */
  printResults(): void {
    const summary = this.getSummary();
    
    console.log('\n📊 MODERN TEST RESULTS');
    console.log('======================');
    console.log(`✅ Passed: ${summary.passed}`);
    console.log(`❌ Failed: ${summary.failed}`);
    console.log(`📈 Success Rate: ${summary.successRate}%`);
    
    if (summary.failed > 0) {
      console.log('\n❌ FAILED TESTS:');
      this.results
        .filter(r => !r.success)
        .forEach(result => {
          console.log(`   - ${result.testName}: ${result.error}`);
        });
    }
    
    const totalTime = this.results.reduce((sum, r) => sum + r.duration, 0);
    console.log(`\n⏱️ Total execution time: ${totalTime}ms`);
  }
  
  /**
   * Sleep utility
   */
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
  
  /**
   * Cleanup all resources
   */
  async cleanup(): Promise<void> {
    if (this.activeClient) {
      await this.clientFactory.cleanupClient(this.activeClient);
      this.activeClient = null;
    }
  }
}

// =============================================================================
// CONVENIENCE FUNCTIONS - Simple interfaces for common patterns
// =============================================================================

/**
 * Quick test execution without managing runner instance
 */
export async function runQuickTest<T>(
  testName: string,
  testFunction: (client: JTAGClient) => Promise<T>,
  options?: ModernTestOptions
): Promise<TestExecutionResult> {
  const runner = new ModernTestRunner();
  try {
    return await runner.executeTest(testName, testFunction, options);
  } finally {
    await runner.cleanup();
  }
}

/**
 * Quick screenshot test
 */
export async function takeTestScreenshot(
  testName: string,
  selector?: string,
  options?: ScreenshotTestOptions
): Promise<TestExecutionResult> {
  const runner = new ModernTestRunner();
  try {
    return await runner.executeScreenshotTest(
      testName,
      selector || MODERN_TEST_CONSTANTS.SELECTORS.BODY,
      options
    );
  } finally {
    await runner.cleanup();
  }
}

/**
 * Test all themes with screenshots
 */
export async function testAllThemes(options?: ModernTestOptions): Promise<ThemeTestResult[]> {
  const runner = new ModernTestRunner();
  const results: ThemeTestResult[] = [];
  
  try {
    console.log('🎨 Testing all themes with screenshots...');
    
    for (const theme of MODERN_TEST_CONSTANTS.THEMES.ALL_THEMES) {
      const result = await runner.executeThemeTest(theme, options);
      results.push(result);
    }
    
    const successful = results.filter(r => r.success).length;
    const total = results.length;
    console.log(`\n🎨 Theme testing complete: ${successful}/${total} themes successful`);
    
    return results;
    
  } finally {
    await runner.cleanup();
  }
}

/**
 * Assert test result with proper error messaging
 */
export function assertTestSuccess(result: TestExecutionResult, context?: string): void {
  if (!result.success) {
    const message = context 
      ? `${context}: ${result.testName} failed - ${result.error}`
      : `Test failed: ${result.testName} - ${result.error}`;
    throw new Error(message);
  }
}

/**
 * Assert all results successful
 */
export function assertAllTestsSuccess(results: TestExecutionResult[], context?: string): void {
  const failed = results.filter(r => !r.success);
  if (failed.length > 0) {
    const message = context
      ? `${context}: ${failed.length}/${results.length} tests failed`
      : `${failed.length}/${results.length} tests failed`;
    
    const firstFailure = failed[0];
    throw new Error(`${message}. First failure: ${firstFailure.testName} - ${firstFailure.error}`);
  }
}

// Export singleton runner for advanced usage
export const modernTestRunner = new ModernTestRunner();