/**
 * Git Hook Integration Test Runner
 * 
 * Simple integration test that creates validation screenshots and verifies
 * they are properly generated with reasonable file sizes.
 */

import { GitHookValidationScreenshot } from './GitHookValidationScreenshot';
import { ContinuumContext } from '../types/shared/core/ContinuumTypes';

export interface GitHookTestResult {
  success: boolean;
  screenshotPath?: string;
  fileSize?: number;
  validationId?: string;
  error?: string;
}

export class GitHookIntegrationTest {
  private context: ContinuumContext;
  private validator: GitHookValidationScreenshot;

  constructor(context: ContinuumContext) {
    this.context = context;
    this.validator = new GitHookValidationScreenshot(context);
  }

  /**
   * Run basic git hook validation test
   */
  async runBasicValidationTest(): Promise<GitHookTestResult> {
    try {
      console.log('🔍 Running git hook validation test...');
      
      const commitHash = await this.getCurrentCommitHash();
      const screenshotPath = await this.validator.createValidationScreenshot({
        commitHash,
        hookType: 'pre-commit',
        testDescription: 'basic-validation-test',
        sessionId: this.context.sessionId
      });

      // Verify screenshot exists and has reasonable size
      const fs = await import('fs');
      const stats = await fs.promises.stat(screenshotPath);
      const fileSizeKB = stats.size / 1024;
      
      // Check file size is reasonable (between 10KB and 5MB)
      const isReasonableSize = fileSizeKB >= 10 && fileSizeKB <= 5120;
      
      if (!isReasonableSize) {
        return {
          success: false,
          screenshotPath,
          fileSize: fileSizeKB,
          error: `Screenshot size ${fileSizeKB.toFixed(2)}KB is outside reasonable range (10KB - 5MB)`
        };
      }

      console.log(`✅ Screenshot created successfully: ${screenshotPath}`);
      console.log(`📏 File size: ${fileSizeKB.toFixed(2)}KB`);
      
      return {
        success: true,
        screenshotPath,
        fileSize: fileSizeKB,
        validationId: this.validator['validationId']
      };
    } catch (error) {
      return {
        success: false,
        error: `Test failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Run chat interaction test
   */
  async runChatInteractionTest(): Promise<GitHookTestResult> {
    try {
      console.log('💬 Running chat interaction test...');
      
      const commitHash = await this.getCurrentCommitHash();
      const screenshots = await this.validator.testChatInteraction({
        commitHash,
        hookType: 'pre-commit',
        sessionId: this.context.sessionId,
        chatMessage: 'Test message for validation',
        targetWidget: 'chat-widget'
      });

      // Verify both screenshots exist
      const fs = await import('fs');
      const beforeStats = await fs.promises.stat(screenshots.before);
      const afterStats = await fs.promises.stat(screenshots.after);
      
      const beforeSizeKB = beforeStats.size / 1024;
      const afterSizeKB = afterStats.size / 1024;
      
      // Check both files have reasonable sizes
      const isBeforeReasonable = beforeSizeKB >= 10 && beforeSizeKB <= 5120;
      const isAfterReasonable = afterSizeKB >= 10 && afterSizeKB <= 5120;
      
      if (!isBeforeReasonable || !isAfterReasonable) {
        return {
          success: false,
          error: `Screenshot sizes not reasonable: before=${beforeSizeKB.toFixed(2)}KB, after=${afterSizeKB.toFixed(2)}KB`
        };
      }

      console.log(`✅ Chat interaction screenshots created successfully`);
      console.log(`📏 Before: ${beforeSizeKB.toFixed(2)}KB, After: ${afterSizeKB.toFixed(2)}KB`);
      
      return {
        success: true,
        screenshotPath: screenshots.before,
        fileSize: beforeSizeKB,
        validationId: this.validator['validationId']
      };
    } catch (error) {
      return {
        success: false,
        error: `Chat test failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Run verified UI component selector tests
   */
  async runVerifiedSelectorTests(): Promise<GitHookTestResult[]> {
    const results: GitHookTestResult[] = [];
    
    // Test verified selectors that exist in the DOM
    const verifiedSelectors = [
      { selector: 'chat-widget', description: 'Chat widget custom element' },
      { selector: 'continuum-sidebar', description: 'Sidebar custom element' },
      { selector: 'div', description: 'First div element' },
      { selector: 'body', description: 'Full page body' },
      { selector: '.app-container', description: 'Main app container' }
    ];

    const commitHash = await this.getCurrentCommitHash();
    
    for (const { selector, description } of verifiedSelectors) {
      try {
        console.log(`🎯 Testing selector: ${selector} (${description})`);
        
        const screenshots = await this.validator.testWidgetInteraction({
          commitHash,
          hookType: 'pre-commit',
          sessionId: this.context.sessionId,
          widgetSelector: selector,
          interactionType: 'click'
        });

        const fs = await import('fs');
        const afterStats = await fs.promises.stat(screenshots.after);
        const fileSizeKB = afterStats.size / 1024;
        
        console.log(`✅ ${selector} screenshot: ${fileSizeKB.toFixed(2)}KB`);
        
        results.push({
          success: true,
          screenshotPath: screenshots.after,
          fileSize: fileSizeKB,
          validationId: this.validator['validationId']
        });
      } catch (error) {
        console.log(`❌ ${selector} failed: ${error instanceof Error ? error.message : String(error)}`);
        results.push({
          success: false,
          error: `Selector ${selector} failed: ${error instanceof Error ? error.message : String(error)}`
        });
      }
    }
    
    return results;
  }

  /**
   * Run widget interaction test
   */
  async runWidgetInteractionTest(): Promise<GitHookTestResult> {
    try {
      console.log('🎛️ Running widget interaction test...');
      
      const commitHash = await this.getCurrentCommitHash();
      const screenshots = await this.validator.testWidgetInteraction({
        commitHash,
        hookType: 'pre-commit',
        sessionId: this.context.sessionId,
        widgetSelector: 'body',
        interactionType: 'click'
      });

      // Verify both screenshots exist
      const fs = await import('fs');
      const beforeStats = await fs.promises.stat(screenshots.before);
      const afterStats = await fs.promises.stat(screenshots.after);
      
      const beforeSizeKB = beforeStats.size / 1024;
      const afterSizeKB = afterStats.size / 1024;
      
      // Check both files have reasonable sizes
      const isBeforeReasonable = beforeSizeKB >= 10 && beforeSizeKB <= 5120;
      const isAfterReasonable = afterSizeKB >= 10 && afterSizeKB <= 5120;
      
      if (!isBeforeReasonable || !isAfterReasonable) {
        return {
          success: false,
          error: `Screenshot sizes not reasonable: before=${beforeSizeKB.toFixed(2)}KB, after=${afterSizeKB.toFixed(2)}KB`
        };
      }

      console.log(`✅ Widget interaction screenshots created successfully`);
      console.log(`📏 Before: ${beforeSizeKB.toFixed(2)}KB, After: ${afterSizeKB.toFixed(2)}KB`);
      
      return {
        success: true,
        screenshotPath: screenshots.before,
        fileSize: beforeSizeKB,
        validationId: this.validator['validationId']
      };
    } catch (error) {
      return {
        success: false,
        error: `Widget test failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Run complete integration test suite
   */
  async runCompleteTestSuite(): Promise<{
    basic: GitHookTestResult;
    chat: GitHookTestResult;
    widget: GitHookTestResult;
    overall: boolean;
  }> {
    console.log('🚀 Running complete git hook integration test suite...');
    
    const basic = await this.runBasicValidationTest();
    const chat = await this.runChatInteractionTest();
    const widget = await this.runWidgetInteractionTest();
    
    const overall = basic.success && chat.success && widget.success;
    
    console.log(`\n📊 Test Results:`);
    console.log(`  Basic validation: ${basic.success ? '✅' : '❌'}`);
    console.log(`  Chat interaction: ${chat.success ? '✅' : '❌'}`);
    console.log(`  Widget interaction: ${widget.success ? '✅' : '❌'}`);
    console.log(`  Overall: ${overall ? '✅' : '❌'}`);
    
    if (!overall) {
      console.log('\n❌ Failures:');
      if (!basic.success) console.log(`  Basic: ${basic.error}`);
      if (!chat.success) console.log(`  Chat: ${chat.error}`);
      if (!widget.success) console.log(`  Widget: ${widget.error}`);
    }
    
    return { basic, chat, widget, overall };
  }

  /**
   * Get current git commit hash
   */
  private async getCurrentCommitHash(): Promise<string> {
    try {
      const { execSync } = await import('child_process');
      return execSync('git rev-parse HEAD', { encoding: 'utf-8' }).trim();
    } catch (error) {
      console.warn('Could not get git commit hash, using fallback');
      return `fallback-${Date.now()}`;
    }
  }

  /**
   * Verify validation directory structure exists
   */
  async verifyValidationDirectoryStructure(): Promise<boolean> {
    try {
      const fs = await import('fs');
      const path = await import('path');
      
      // Use the validation directory path provided by session context
      const validationDir = this.context.sessionPaths?.base || '.continuum/sessions';
      const fullPath = path.resolve(`${validationDir}/validation`);
      
      // Check if directory exists - it should be created automatically
      const stats = await fs.promises.stat(fullPath);
      
      return stats.isDirectory();
    } catch (error) {
      console.warn('Session validation directory structure not found:', error);
      return false;
    }
  }

  /**
   * Clean up test artifacts (optional)
   */
  async cleanupTestArtifacts(): Promise<void> {
    try {
      const fs = await import('fs');
      const path = await import('path');
      
      // Use the validation directory path provided by session context
      const validationDir = this.context.sessionPaths?.base || '.continuum/sessions';
      const fullPath = path.resolve(`${validationDir}/validation`);
      
      // Only clean up if directory exists
      if (await fs.promises.access(fullPath).then(() => true).catch(() => false)) {
        const files = await fs.promises.readdir(fullPath);
        
        // Remove test files (keep validation structure)
        for (const file of files) {
          if (file.includes('test') || file.includes('fallback')) {
            await fs.promises.unlink(path.join(fullPath, file));
          }
        }
      }
    } catch (error) {
      console.warn('Could not clean up test artifacts:', error);
    }
  }
}