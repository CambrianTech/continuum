/**
 * Browser Test Session - High-level encapsulation for browser UI testing
 * 
 * Provides a clean, encapsulated interface for browser UI testing scenarios.
 * Handles common patterns like chat interactions, form filling, and UI verification.
 */

import type { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import {
  interactWithBrowserUI,
  sendChatMessageThroughUI,
  screenshotUIComponent,
  waitForUIElement,
  verifyChatMessageInUI
} from './BrowserUITestHelpers';
import type { BrowserUIResult } from './BrowserUITestHelpers';

export interface BrowserTestSessionConfig {
  controllingClient: JTAGClient;
  sessionName: string;
  screenshotPrefix?: string;
}

export interface ChatTestScenario {
  participants: Array<{
    name: string;
    messages: string[];
  }>;
  roomId?: string;
  verifyInUI?: boolean;
  takeScreenshots?: boolean;
}

/**
 * High-level browser test session with encapsulated common patterns
 */
export class BrowserTestSession {
  private client: JTAGClient;
  private sessionName: string;
  private screenshotPrefix: string;
  private screenshotCount = 0;

  constructor(config: BrowserTestSessionConfig) {
    this.client = config.controllingClient;
    this.sessionName = config.sessionName;
    this.screenshotPrefix = config.screenshotPrefix || config.sessionName;
  }

  /**
   * Execute a complete chat test scenario
   */
  async executeChatScenario(scenario: ChatTestScenario): Promise<BrowserUIResult> {
    console.log(`🎬 BROWSER SESSION: Starting chat scenario - ${this.sessionName}`);

    try {
      // Take initial screenshot
      if (scenario.takeScreenshots) {
        await this.screenshot('chat-widget', 'initial-state');
      }

      // Execute conversation
      for (const participant of scenario.participants) {
        for (const message of participant.messages) {
          // Send message through UI
          const sendResult = await sendChatMessageThroughUI(
            this.client, 
            message, 
            participant.name
          );

          if (!sendResult.success) {
            return { success: false, error: `Failed to send message: ${sendResult.error}` };
          }

          // Wait for UI to update
          await this.wait(800);

          // Verify message appears if requested
          if (scenario.verifyInUI) {
            const verifyResult = await verifyChatMessageInUI(
              this.client,
              message,
              participant.name
            );

            if (!verifyResult.success) {
              console.log(`⚠️ BROWSER SESSION: Message verification failed for ${participant.name}`);
            }
          }

          // Take screenshot after each message
          if (scenario.takeScreenshots) {
            await this.screenshot('chat-widget', `after-${participant.name.toLowerCase()}-message`);
          }
        }
      }

      console.log(`✅ BROWSER SESSION: Chat scenario completed - ${this.sessionName}`);
      return { success: true };

    } catch (error) {
      console.error(`❌ BROWSER SESSION: Chat scenario failed - ${this.sessionName}:`, error);
      return { success: false, error: String(error) };
    }
  }

  /**
   * Fill out a form in the browser UI
   */
  async fillForm(formFields: Array<{ selector: string; value: string }>): Promise<BrowserUIResult> {
    console.log(`📝 BROWSER SESSION: Filling form with ${formFields.length} fields`);

    for (const field of formFields) {
      const result = await interactWithBrowserUI(this.client, {
        selector: field.selector,
        action: 'type',
        value: field.value
      });

      if (!result.success) {
        return { success: false, error: `Failed to fill field ${field.selector}: ${result.error}` };
      }
    }

    return { success: true };
  }

  /**
   * Click a sequence of buttons/elements
   */
  async clickSequence(selectors: string[], waitBetween: number = 500): Promise<BrowserUIResult> {
    console.log(`👆 BROWSER SESSION: Clicking sequence of ${selectors.length} elements`);

    for (const selector of selectors) {
      const result = await interactWithBrowserUI(this.client, {
        selector,
        action: 'click'
      });

      if (!result.success) {
        return { success: false, error: `Failed to click ${selector}: ${result.error}` };
      }

      if (waitBetween > 0) {
        await this.wait(waitBetween);
      }
    }

    return { success: true };
  }

  /**
   * Wait for multiple elements to appear
   */
  async waitForElements(selectors: string[], timeout: number = 10000): Promise<BrowserUIResult> {
    console.log(`⏳ BROWSER SESSION: Waiting for ${selectors.length} elements`);

    for (const selector of selectors) {
      const result = await waitForUIElement(this.client, selector, timeout);
      if (!result.success) {
        return { success: false, error: `Element not found: ${selector}` };
      }
    }

    return { success: true };
  }

  /**
   * Take screenshot with automatic naming
   */
  async screenshot(selector: string, suffix?: string): Promise<void> {
    this.screenshotCount++;
    const filename = `${this.screenshotPrefix}-${String(this.screenshotCount).padStart(2, '0')}${suffix ? `-${suffix}` : ''}.png`;
    
    await screenshotUIComponent(this.client, selector, filename);
  }

  /**
   * Execute custom browser JavaScript with error handling
   */
  async executeScript(script: string, description?: string): Promise<BrowserUIResult> {
    console.log(`🔧 BROWSER SESSION: ${description || 'Executing custom script'}`);

    try {
      const result = await this.client.commands.exec({
        code: {
          type: 'inline',
          language: 'javascript',
          source: script
        }
      });

      if (result.success && result.commandResult?.success) {
        const execResult = result.commandResult.result || result.commandResult.commandResult;
        return execResult || { success: true };
      } else {
        return { success: false, error: 'Script execution failed' };
      }
    } catch (error) {
      return { success: false, error: String(error) };
    }
  }

  /**
   * Simple wait helper
   */
  private async wait(ms: number): Promise<void> {
    await new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Get session summary
   */
  getSessionSummary(): string {
    return `Browser Test Session: ${this.sessionName} (${this.screenshotCount} screenshots taken)`;
  }
}