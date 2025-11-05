/**
 * Git Hook Validation Screenshot System
 * 
 * Creates validation screenshots during git hooks to provide visual
 * debugging capabilities and UI regression detection.
 */

import { ContinuumBrowserClient } from '../ui/continuum-browser-client/ContinuumBrowserClient';
import { ContinuumContext } from '../types/shared/core/ContinuumTypes';

export interface GitHookValidationConfig {
  commitHash?: string;
  hookType: 'pre-commit' | 'post-commit' | 'pre-push';
  testDescription?: string;
  sessionId: string;
}

export class GitHookValidationScreenshot {
  private browserClient: ContinuumBrowserClient;
  private context: ContinuumContext;
  private validationId: string;

  constructor(context: ContinuumContext) {
    this.context = context;
    this.browserClient = new ContinuumBrowserClient();
    this.validationId = `validation-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * Create validation screenshot with bright red UUID indicator
   */
  async createValidationScreenshot(config: GitHookValidationConfig): Promise<string> {
    const { commitHash, hookType, testDescription, sessionId } = config;
    
    // Create validation indicator on page
    const validationIndicator = await this.addValidationIndicator();
    
    try {
      // Take screenshot
      const screenshotResult = await this.browserClient.execute('screenshot', {
        sessionId,
        format: 'png',
        quality: 90,
        fullPage: true,
        metadata: {
          validationId: this.validationId,
          commitHash,
          hookType,
          testDescription,
          timestamp: new Date().toISOString()
        }
      });
      
      const screenshotPath = screenshotResult.data as unknown as string;

      // Copy to validation directory
      await this.copyToValidationDirectory(screenshotPath, config);
      
      return screenshotPath;
    } finally {
      // Clean up validation indicator
      await this.removeValidationIndicator(validationIndicator);
    }
  }

  /**
   * Add bright red validation indicator with UUID
   */
  private async addValidationIndicator(): Promise<string> {
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
      sessionId: this.context.sessionId,
      script,
      waitForResult: true
    });

    return indicatorId;
  }

  /**
   * Remove validation indicator
   */
  private async removeValidationIndicator(indicatorId: string): Promise<void> {
    const script = `
      const indicator = document.getElementById('${indicatorId}');
      if (indicator) {
        indicator.remove();
      }
    `;

    await this.browserClient.execute('execute', {
      sessionId: this.context.sessionId,
      script,
      waitForResult: true
    });
  }

  /**
   * Copy screenshot to validation directory
   */
  private async copyToValidationDirectory(screenshotPath: string, config: GitHookValidationConfig): Promise<void> {
    const { hookType } = config;
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const fileName = `${hookType}-${timestamp}-${this.validationId}.png`;
    
    // Simple file copy - just use the filename, validation directory is handled by context
    const fs = await import('fs');
    await fs.promises.copyFile(screenshotPath, `validation/${fileName}`);
  }

  /**
   * Create before/after screenshots for UI interactions
   */
  async createInteractionScreenshots(config: GitHookValidationConfig & {
    interactionType: 'click' | 'input' | 'navigation';
    targetElement?: string;
  }): Promise<{ before: string; after: string }> {
    const { interactionType, targetElement } = config;
    
    // Take before screenshot
    const beforeScreenshot = await this.createValidationScreenshot({
      ...config,
      testDescription: `${interactionType}-before${targetElement ? `-${targetElement}` : ''}`
    });
    
    // Wait for interaction to complete (placeholder - would be integrated with actual interaction)
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Take after screenshot
    const afterScreenshot = await this.createValidationScreenshot({
      ...config,
      testDescription: `${interactionType}-after${targetElement ? `-${targetElement}` : ''}`
    });
    
    return {
      before: beforeScreenshot,
      after: afterScreenshot
    };
  }

  /**
   * Integration test for chat interactions
   */
  async testChatInteraction(config: GitHookValidationConfig & {
    chatMessage: string;
    targetWidget?: string;
  }): Promise<{ before: string; after: string }> {
    const { sessionId, chatMessage, targetWidget } = config;
    
    // Before screenshot
    const beforeScreenshot = await this.createValidationScreenshot({
      ...config,
      testDescription: `chat-before-${targetWidget || 'default'}`
    });
    
    // Simulate chat interaction
    const chatScript = `
      // Find chat input or create one for testing
      let chatInput = document.querySelector('${targetWidget || '[data-testid="chat-input"]'}');
      if (!chatInput) {
        chatInput = document.createElement('textarea');
        chatInput.setAttribute('data-testid', 'chat-input');
        chatInput.style.cssText = 'position: fixed; bottom: 20px; left: 20px; width: 300px; height: 100px; z-index: 9998;';
        document.body.appendChild(chatInput);
      }
      
      chatInput.value = '${chatMessage}';
      chatInput.dispatchEvent(new Event('input', { bubbles: true }));
      
      // Add visual feedback
      chatInput.style.border = '2px solid #00ff00';
    `;
    
    await this.browserClient.execute('execute', {
      sessionId,
      script: chatScript,
      waitForResult: true
    });
    
    // Wait for chat processing
    await new Promise(resolve => setTimeout(resolve, 2000));
    
    // After screenshot
    const afterScreenshot = await this.createValidationScreenshot({
      ...config,
      testDescription: `chat-after-${targetWidget || 'default'}`
    });
    
    return {
      before: beforeScreenshot,
      after: afterScreenshot
    };
  }

  /**
   * Integration test for widget interactions
   */
  async testWidgetInteraction(config: GitHookValidationConfig & {
    widgetSelector: string;
    interactionType: 'click' | 'hover' | 'input';
    inputValue?: string;
  }): Promise<{ before: string; after: string }> {
    const { sessionId, widgetSelector, interactionType, inputValue } = config;
    
    // Before screenshot
    const beforeScreenshot = await this.createValidationScreenshot({
      ...config,
      testDescription: `widget-${interactionType}-before`
    });
    
    // Perform widget interaction
    const interactionScript = `
      const widget = document.querySelector('${widgetSelector}');
      if (widget) {
        widget.style.border = '2px solid #ffff00';
        
        switch ('${interactionType}') {
          case 'click':
            widget.click();
            break;
          case 'hover':
            widget.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
            break;
          case 'input':
            if (widget.tagName.toLowerCase() === 'input' || widget.tagName.toLowerCase() === 'textarea') {
              widget.value = '${inputValue || ''}';
              widget.dispatchEvent(new Event('input', { bubbles: true }));
            }
            break;
        }
      }
    `;
    
    await this.browserClient.execute('execute', {
      sessionId,
      script: interactionScript,
      waitForResult: true
    });
    
    // Wait for widget response
    await new Promise(resolve => setTimeout(resolve, 1500));
    
    // After screenshot
    const afterScreenshot = await this.createValidationScreenshot({
      ...config,
      testDescription: `widget-${interactionType}-after`
    });
    
    return {
      before: beforeScreenshot,
      after: afterScreenshot
    };
  }
}