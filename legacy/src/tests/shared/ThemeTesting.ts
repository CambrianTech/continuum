/**
 * Theme Testing - Specialized utilities for theme system validation
 * 
 * Provides theme switching, validation, and screenshot capture utilities
 * for comprehensive theme system testing.
 */

import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import { JTAGClientFactory } from './JTAGClientFactory';
import { THEME_CATALOG, ThemeName, DOM_SELECTORS, TestTimeout, TEST_TIMEOUTS } from './TestConstants';
import { ScreenshotTestingEngine, ScreenshotResult } from './ScreenshotTesting';

// =============================================================================
// THEME TESTING INTERFACES - Theme-specific test types
// =============================================================================

export interface ThemeTestOptions {
  timeout?: TestTimeout;
  validateSwitch?: boolean;
  captureScreenshot?: boolean;
  screenshotSelector?: string;
  logProgress?: boolean;
}

export interface ThemeTestResult {
  theme: ThemeName;
  success: boolean;
  screenshotResult?: ScreenshotResult;
  error?: string;
  switchTime?: number;
  screenshotTime?: number;
  totalTime: number;
}

export interface ThemeSuiteResult {
  themes: ThemeTestResult[];
  summary: {
    successful: number;
    failed: number;
    total: number;
    successRate: number;
    totalDuration: number;
  };
}

// =============================================================================
// THEME TESTING ENGINE - Comprehensive theme validation
// =============================================================================

export class ThemeTestingEngine {
  private clientFactory: JTAGClientFactory;
  private screenshotEngine: ScreenshotTestingEngine;
  
  constructor() {
    this.clientFactory = JTAGClientFactory.getInstance();
    this.screenshotEngine = new ScreenshotTestingEngine();
  }
  
  /**
   * Get available themes from theme system
   */
  async getAvailableThemes(client: JTAGClient): Promise<ThemeName[]> {
    console.log('📋 ThemeTesting: Getting available themes...');
    
    try {
      const result = await this.clientFactory.executeCommand(
        client,
        'theme/list',
        {},
        { timeout: TEST_TIMEOUTS.STANDARD_OPERATION }
      );
      
      if (!result.success) {
        console.warn('⚠️ ThemeTesting: theme/list failed, using fallback theme catalog');
        return [...THEME_CATALOG.ALL_THEMES];
      }
      
      const themes = result.data?.themes || THEME_CATALOG.ALL_THEMES;
      console.log(`✅ ThemeTesting: Found ${themes.length} themes: ${themes.join(', ')}`);
      
      return themes;
      
    } catch (error) {
      console.warn('⚠️ ThemeTesting: Error getting themes, using fallback:', error);
      return [...THEME_CATALOG.ALL_THEMES];
    }
  }
  
  /**
   * Switch to specific theme with timing and validation
   */
  async switchTheme(
    client: JTAGClient, 
    themeName: ThemeName,
    options: ThemeTestOptions = {}
  ): Promise<{ success: boolean; switchTime: number; error?: string }> {
    const startTime = Date.now();
    
    console.log(`🎨 ThemeTesting: Switching to theme "${themeName}"`);
    
    try {
      const result = await this.clientFactory.executeCommand(
        client,
        'theme/set',
        { themeName },
        { timeout: options.timeout || TEST_TIMEOUTS.STANDARD_OPERATION }
      );
      
      if (!result.success) {
        throw new Error(`Theme switch failed: ${result.error}`);
      }
      
      // Wait for theme to apply
      await this.sleep(THEME_CATALOG.THEME_SWITCH_DELAY);
      
      const switchTime = Date.now() - startTime;
      
      console.log(`✅ ThemeTesting: Theme "${themeName}" applied (${switchTime}ms)`);
      
      return {
        success: true,
        switchTime
      };
      
    } catch (error) {
      const switchTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      console.error(`❌ ThemeTesting: Theme "${themeName}" switch failed (${switchTime}ms):`, errorMessage);
      
      return {
        success: false,
        switchTime,
        error: errorMessage
      };
    }
  }
  
  /**
   * Test single theme with optional screenshot capture
   */
  async testSingleTheme(
    client: JTAGClient,
    themeName: ThemeName,
    options: ThemeTestOptions = {}
  ): Promise<ThemeTestResult> {
    const testStartTime = Date.now();
    
    if (options.logProgress !== false) {
      console.log(`🎨 ThemeTesting: Testing theme "${themeName}"`);
    }
    
    try {
      // Switch theme with timing
      const switchResult = await this.switchTheme(client, themeName, options);
      
      if (!switchResult.success) {
        throw new Error(switchResult.error);
      }
      
      // Capture screenshot if requested
      let screenshotResult: ScreenshotResult | undefined;
      if (options.captureScreenshot !== false) {
        screenshotResult = await this.screenshotEngine.captureScreenshot(
          client,
          {
            selector: options.screenshotSelector || DOM_SELECTORS.BODY,
            filename: `theme-${themeName}.png`,
            timeout: options.timeout,
            validateFile: true
          }
        );
        
        if (!screenshotResult.success) {
          throw new Error(`Screenshot failed: ${screenshotResult.error}`);
        }
      }
      
      const totalTime = Date.now() - testStartTime;
      
      if (options.logProgress !== false) {
        console.log(`✅ ThemeTesting: Theme "${themeName}" test completed (${totalTime}ms)`);
      }
      
      return {
        theme: themeName,
        success: true,
        screenshotResult,
        switchTime: switchResult.switchTime,
        screenshotTime: screenshotResult?.captureTime,
        totalTime
      };
      
    } catch (error) {
      const totalTime = Date.now() - testStartTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      console.error(`❌ ThemeTesting: Theme "${themeName}" test failed (${totalTime}ms):`, errorMessage);
      
      return {
        theme: themeName,
        success: false,
        error: errorMessage,
        totalTime
      };
    }
  }
  
  /**
   * Test all available themes with comprehensive reporting
   */
  async testAllThemes(options: ThemeTestOptions = {}): Promise<ThemeSuiteResult> {
    console.log('🎨 ThemeTesting: Starting comprehensive theme test suite');
    
    const suiteStartTime = Date.now();
    
    try {
      // Get client
      const connection = await this.clientFactory.createClient();
      const client = connection.client;
      
      try {
        // Get available themes
        const themes = await this.getAvailableThemes(client);
        
        console.log(`🎨 ThemeTesting: Testing ${themes.length} themes`);
        
        // Test each theme
        const results: ThemeTestResult[] = [];
        for (const themeName of themes) {
          const result = await this.testSingleTheme(client, themeName, options);
          results.push(result);
        }
        
        // Calculate summary
        const successful = results.filter(r => r.success).length;
        const failed = results.filter(r => !r.success).length;
        const total = results.length;
        const successRate = total > 0 ? Math.round((successful / total) * 100) : 0;
        const totalDuration = Date.now() - suiteStartTime;
        
        const suiteResult: ThemeSuiteResult = {
          themes: results,
          summary: {
            successful,
            failed,
            total,
            successRate,
            totalDuration
          }
        };
        
        // Print detailed results
        console.log('\n🎨 THEME TEST SUITE SUMMARY');
        console.log('==========================');
        console.log(`📊 Success Rate: ${successful}/${total} (${successRate}%)`);
        console.log(`⏱️ Total Duration: ${totalDuration}ms`);
        
        for (const result of results) {
          const status = result.success ? '✅' : '❌';
          const details = result.success 
            ? `${result.screenshotResult?.filepath} (switch: ${result.switchTime}ms, capture: ${result.screenshotTime}ms)`
            : result.error;
          console.log(`${status} ${result.theme}: ${details}`);
        }
        
        if (successful === 0) {
          console.error('💥 ThemeTesting: No themes were successfully tested');
        }
        
        return suiteResult;
        
      } finally {
        await this.clientFactory.cleanupClient(client);
      }
      
    } catch (error) {
      const totalDuration = Date.now() - suiteStartTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      console.error(`💥 ThemeTesting: Suite failed (${totalDuration}ms):`, errorMessage);
      
      return {
        themes: [],
        summary: {
          successful: 0,
          failed: 1,
          total: 1,
          successRate: 0,
          totalDuration
        }
      };
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
// CONVENIENCE FUNCTIONS - Simple interfaces for common theme testing patterns
// =============================================================================

/**
 * Quick theme test with screenshot
 */
export async function testThemeWithScreenshot(
  themeName: ThemeName,
  options?: ThemeTestOptions
): Promise<ThemeTestResult> {
  const engine = new ThemeTestingEngine();
  const connection = await engine['clientFactory'].createClient();
  const client = connection.client;
  
  try {
    return await engine.testSingleTheme(client, themeName, {
      captureScreenshot: true,
      ...options
    });
  } finally {
    await engine['clientFactory'].cleanupClient(client);
  }
}

/**
 * Test all themes with screenshots (convenience function)
 */
export async function testAllThemesWithScreenshots(
  options?: ThemeTestOptions
): Promise<ThemeSuiteResult> {
  const engine = new ThemeTestingEngine();
  return engine.testAllThemes({
    captureScreenshot: true,
    validateSwitch: true,
    logProgress: true,
    ...options
  });
}

/**
 * Quick theme switch without testing
 */
export async function switchToTheme(themeName: ThemeName): Promise<boolean> {
  const engine = new ThemeTestingEngine();
  const connection = await engine['clientFactory'].createClient();
  const client = connection.client;
  
  try {
    const result = await engine.switchTheme(client, themeName);
    return result.success;
  } finally {
    await engine['clientFactory'].cleanupClient(client);
  }
}

// Export singleton engine for advanced usage
export const themeTestingEngine = new ThemeTestingEngine();