/**
 * Git Hook Validation Test
 * 
 * Simple integration test for git hook validation screenshot functionality
 * following the intelligent modular testing framework structure.
 */

import { ContinuumBrowserClient } from '../ui/continuum-browser-client/ContinuumBrowserClient';

export interface GitHookValidationResult {
  success: boolean;
  screenshotPath?: string;
  validationId?: string;
  error?: string;
}

export class GitHookValidationTest {
  private browserClient: ContinuumBrowserClient;
  private validationId: string;

  constructor() {
    this.browserClient = new ContinuumBrowserClient();
    this.validationId = `validation-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * Run basic git hook validation test
   */
  async runBasicValidationTest(): Promise<GitHookValidationResult> {
    try {
      console.log('🔍 Running basic git hook validation test...');
      
      // Get current session ID
      const sessionId = this.browserClient.sessionId;
      if (!sessionId) {
        return {
          success: false,
          error: 'No active session found'
        };
      }

      // Add validation indicator
      const indicatorId = await this.addValidationIndicator(sessionId);
      
      try {
        // Take screenshot
        const screenshotResult = await this.browserClient.execute('screenshot', {
          sessionId,
          format: 'png',
          fullPage: true
        });

        if (!screenshotResult.success) {
          return {
            success: false,
            error: `Screenshot failed: ${screenshotResult.error}`
          };
        }

        const screenshotPath = screenshotResult.data as unknown as string;
        
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
            error: `Screenshot size ${fileSizeKB.toFixed(2)}KB is outside reasonable range (10KB - 5MB)`
          };
        }

        console.log(`✅ Screenshot created successfully: ${screenshotPath}`);
        console.log(`📏 File size: ${fileSizeKB.toFixed(2)}KB`);
        
        return {
          success: true,
          screenshotPath,
          validationId: this.validationId
        };
        
      } finally {
        // Clean up validation indicator
        await this.removeValidationIndicator(sessionId, indicatorId);
      }
      
    } catch (error) {
      return {
        success: false,
        error: `Test failed: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Add bright red validation indicator with UUID
   */
  private async addValidationIndicator(sessionId: string): Promise<string> {
    const indicatorId = `validation-indicator-${this.validationId}`;
    
    const script = `
      const indicator = document.createElement('div');
      indicator.id = '${indicatorId}';
      indicator.style.cssText = \`
        position: fixed;
        top: 10px;
        right: 10px;
        background: #ff0000;
        color: white;
        padding: 10px;
        border-radius: 5px;
        font-family: monospace;
        font-size: 12px;
        z-index: 9999;
        box-shadow: 0 2px 10px rgba(0,0,0,0.3);
      \`;
      indicator.textContent = 'GIT HOOK VALIDATION\\n${this.validationId}';
      document.body.appendChild(indicator);
      return '${indicatorId}';
    `;

    await this.browserClient.execute('execute', {
      sessionId,
      script,
      waitForResult: true
    });

    return indicatorId;
  }

  /**
   * Remove validation indicator
   */
  private async removeValidationIndicator(sessionId: string, indicatorId: string): Promise<void> {
    const script = `
      const indicator = document.getElementById('${indicatorId}');
      if (indicator) {
        indicator.remove();
      }
    `;

    await this.browserClient.execute('execute', {
      sessionId,
      script,
      waitForResult: true
    });
  }

  /**
   * Get current git commit hash
   */
  async getCurrentCommitHash(): Promise<string> {
    try {
      const { execSync } = await import('child_process');
      return execSync('git rev-parse HEAD', { encoding: 'utf-8' }).trim();
    } catch (error) {
      console.warn('Could not get git commit hash, using fallback');
      return `fallback-${Date.now()}`;
    }
  }
}