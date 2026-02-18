/**
 * Screenshot Testing - Specialized utilities for visual test validation
 * 
 * Provides screenshot capture, validation, and comparison utilities
 * with proper file handling and visual regression testing support.
 */

import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import { JTAGClientFactory } from './JTAGClientFactory';
import { DOM_SELECTORS, FILE_VALIDATION, COMMAND_DEFAULTS, TestTimeout } from './TestConstants';
import { TestExecutionResult } from './TestExecution';

// =============================================================================
// SCREENSHOT TESTING INTERFACES - Comprehensive visual testing types
// =============================================================================

export interface ScreenshotOptions {
  selector?: string;
  filename?: string;
  validateFile?: boolean;
  includeMetadata?: boolean;
  timeout?: TestTimeout;
  format?: 'png' | 'jpg' | 'jpeg' | 'webp';
  quality?: number;
}

export interface ScreenshotResult {
  success: boolean;
  filepath?: string;
  fileSize?: number;
  dimensions?: { width: number; height: number };
  error?: string;
  captureTime: number;
}

export interface ScreenshotComparison {
  baseline: string;
  current: string;
  match: boolean;
  difference?: number;
  diffImage?: string;
}

// =============================================================================
// SCREENSHOT TESTING ENGINE - Visual validation utilities
// =============================================================================

export class ScreenshotTestingEngine {
  private clientFactory: JTAGClientFactory;
  
  constructor() {
    this.clientFactory = JTAGClientFactory.getInstance();
  }
  
  /**
   * Capture screenshot with comprehensive validation
   */
  async captureScreenshot(
    client: JTAGClient,
    options: ScreenshotOptions = {}
  ): Promise<ScreenshotResult> {
    const startTime = Date.now();
    const selector = options.selector || DOM_SELECTORS.BODY;
    const filename = options.filename || `screenshot-${Date.now()}`;
    
    console.log(`📸 ScreenshotTesting: Capturing "${selector}" → "${filename}"`);
    
    try {
      const result = await this.clientFactory.executeCommand(
        client,
        'screenshot',
        { 
          querySelector: selector,
          filename,
          format: options.format || COMMAND_DEFAULTS.SCREENSHOT.FORMAT,
          quality: options.quality || COMMAND_DEFAULTS.SCREENSHOT.QUALITY
        },
        { timeout: options.timeout }
      );
      
      if (!result.success) {
        throw new Error(`Screenshot command failed: ${result.error}`);
      }
      
      const captureTime = Date.now() - startTime;
      
      // Validate file if requested
      if (options.validateFile !== false && result.data?.filepath) {
        const fileValid = this.validateScreenshotFile(result.data.filepath);
        if (!fileValid) {
          throw new Error(`Screenshot file validation failed: ${result.data.filepath}`);
        }
      }
      
      const screenshotResult: ScreenshotResult = {
        success: true,
        filepath: result.data?.filepath,
        fileSize: result.data?.fileSize,
        dimensions: result.data?.dimensions,
        captureTime
      };
      
      console.log(`✅ ScreenshotTesting: Captured successfully (${captureTime}ms)`);
      return screenshotResult;
      
    } catch (error) {
      const captureTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      console.error(`❌ ScreenshotTesting: Capture failed (${captureTime}ms):`, errorMessage);
      
      return {
        success: false,
        error: errorMessage,
        captureTime
      };
    }
  }
  
  /**
   * Execute screenshot test with proper validation
   */
  async executeScreenshotTest(
    testName: string,
    selector: string = DOM_SELECTORS.BODY,
    options: ScreenshotOptions = {}
  ): Promise<TestExecutionResult> {
    const startTime = Date.now();
    
    try {
      // Get client
      const connection = await this.clientFactory.createClient();
      const client = connection.client;
      
      try {
        // Capture screenshot
        const screenshotResult = await this.captureScreenshot(client, {
          selector,
          filename: options.filename || `${testName.toLowerCase().replace(/\s+/g, '-')}-${Date.now()}`,
          ...options
        });
        
        if (!screenshotResult.success) {
          throw new Error(`Screenshot failed: ${screenshotResult.error}`);
        }
        
        const duration = Date.now() - startTime;
        
        return {
          testName,
          success: true,
          duration,
          data: screenshotResult,
          screenshots: screenshotResult.filepath ? [screenshotResult.filepath] : []
        };
        
      } finally {
        await this.clientFactory.cleanupClient(client);
      }
      
    } catch (error) {
      const duration = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      return {
        testName,
        success: false,
        duration,
        error: errorMessage
      };
    }
  }
  
  /**
   * Capture before/after screenshot pair for comparison
   */
  async captureBeforeAfterScreenshots(
    client: JTAGClient,
    testName: string,
    actionFunction: () => Promise<void>,
    options: ScreenshotOptions = {}
  ): Promise<{ before: ScreenshotResult; after: ScreenshotResult }> {
    console.log(`📸 ScreenshotTesting: Capturing before/after for "${testName}"`);
    
    // Capture before screenshot
    const beforeResult = await this.captureScreenshot(client, {
      ...options,
      filename: `${testName}-before`
    });
    
    if (!beforeResult.success) {
      throw new Error(`Before screenshot failed: ${beforeResult.error}`);
    }
    
    // Execute action
    console.log(`⚡ ScreenshotTesting: Executing action for "${testName}"`);
    await actionFunction();
    
    // Wait for changes to settle
    await this.sleep(500);
    
    // Capture after screenshot  
    const afterResult = await this.captureScreenshot(client, {
      ...options,
      filename: `${testName}-after`
    });
    
    if (!afterResult.success) {
      throw new Error(`After screenshot failed: ${afterResult.error}`);
    }
    
    console.log(`✅ ScreenshotTesting: Before/after capture completed for "${testName}"`);
    
    return {
      before: beforeResult,
      after: afterResult
    };
  }
  
  /**
   * Validate screenshot file meets requirements
   */
  private validateScreenshotFile(filepath: string): boolean {
    try {
      const fs = require('fs');
      const stats = fs.statSync(filepath);
      
      const minSize = FILE_VALIDATION.MIN_SCREENSHOT_SIZE;
      const maxSize = FILE_VALIDATION.MAX_REASONABLE_SCREENSHOT;
      
      if (stats.size < minSize) {
        console.warn(`⚠️ ScreenshotTesting: File too small: ${stats.size} bytes (min: ${minSize})`);
        return false;
      }
      
      if (stats.size > maxSize) {
        console.warn(`⚠️ ScreenshotTesting: File too large: ${stats.size} bytes (max: ${maxSize})`);
        return false;
      }
      
      console.log(`✅ ScreenshotTesting: File valid: ${stats.size} bytes`);
      return true;
      
    } catch (error) {
      console.error(`❌ ScreenshotTesting: Failed to validate file "${filepath}":`, error);
      return false;
    }
  }
  
  /**
   * Sleep utility
   */
  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}

// =============================================================================
// CONVENIENCE FUNCTIONS - Simple interfaces for common screenshot patterns
// =============================================================================

/**
 * Quick screenshot capture
 */
export async function captureTestScreenshot(
  testName: string,
  selector?: string,
  options?: ScreenshotOptions
): Promise<TestExecutionResult> {
  const engine = new ScreenshotTestingEngine();
  return engine.executeScreenshotTest(
    testName,
    selector || DOM_SELECTORS.BODY,
    options
  );
}

/**
 * Multi-selector screenshot batch
 */
export async function captureMultipleScreenshots(
  testName: string,
  selectors: Array<{ name: string; selector: string }>,
  options?: ScreenshotOptions
): Promise<TestExecutionResult[]> {
  const engine = new ScreenshotTestingEngine();
  const results: TestExecutionResult[] = [];
  
  for (const { name, selector } of selectors) {
    const result = await engine.executeScreenshotTest(
      `${testName} - ${name}`,
      selector,
      options
    );
    results.push(result);
  }
  
  return results;
}

/**
 * Full page screenshot with common selectors
 */
export async function capturePageScreenshots(
  testName: string,
  options?: ScreenshotOptions
): Promise<TestExecutionResult[]> {
  return captureMultipleScreenshots(
    testName,
    [
      { name: 'Full Page', selector: DOM_SELECTORS.BODY },
      { name: 'Chat Widget', selector: DOM_SELECTORS.CHAT_WIDGET },
      { name: 'Sidebar', selector: DOM_SELECTORS.SIDEBAR }
    ],
    options
  );
}

// Export singleton engine for advanced usage
export const screenshotTestingEngine = new ScreenshotTestingEngine();