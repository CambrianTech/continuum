/**
 * Screenshot Handler - Belongs with Screenshot Command
 * ===================================================
 * Handler for screenshot command - proper module ownership
 */

import { StrongScreenshotParams, ScreenshotTypeGuards } from './ScreenshotTypes';
import { ScreenshotDestination } from './ScreenshotCommand';

export type ScreenshotAction = 'screenshot' | 'capture' | 'snap';

export class ScreenshotHandler {
  // Strong-typed action handler mapping - single source of truth
  private readonly actionHandlers: Record<ScreenshotAction, (params: StrongScreenshotParams) => Promise<void>> = {
    screenshot: (params) => this.takeScreenshot(params),
    capture: (params) => this.captureElement(params),
    snap: (params) => this.quickSnapshot(params)
  } as const;

  /**
   * Handle screenshot-related actions with strong types
   */
  async handle(action: string, params: Record<string, unknown>): Promise<void> {
    // Type-safe action validation
    if (!this.isValidAction(action)) {
      throw new Error(`Unsupported action: ${action}`);
    }

    // Type-safe parameter conversion
    const typedParams = ScreenshotTypeGuards.validateAndConvertParams(params);

    // Execute with strong typing - no switch statement needed
    const handler = this.actionHandlers[action];
    await handler(typedParams);
  }

  /**
   * Type guard for action validation - uses handler mapping as single source of truth
   */
  private isValidAction(action: string): action is ScreenshotAction {
    return action in this.actionHandlers;
  }


  /**
   * Get supported actions - derived from handler mapping (single source of truth)
   */
  getActions(): readonly ScreenshotAction[] {
    return Object.keys(this.actionHandlers) as ScreenshotAction[];
  }

  /**
   * Take full page screenshot with strong types
   */
  private async takeScreenshot(params: StrongScreenshotParams): Promise<void> {
    const filename = params.filename || `screenshot-${Date.now()}.png`;
    
    console.log('📸 Taking screenshot:', { filename, params });
    
    // Delegate to the actual ScreenshotCommand in this same directory
    const { ScreenshotCommand } = await import('./ScreenshotCommand');
    
    const screenshotParams: any = {
      filename,
      destination: ScreenshotDestination.FILE
    };
    
    // Only add selector if it exists (exactOptionalPropertyTypes compliance)
    if (params.selector) {
      screenshotParams.selector = params.selector;
    }
    
    await ScreenshotCommand.execute(screenshotParams);
  }

  /**
   * Capture specific element with strong types
   */
  private async captureElement(params: StrongScreenshotParams): Promise<void> {
    const selector = params.selector || 'body';
    const filename = params.filename || `element-${Date.now()}.png`;
    
    console.log('📸 Capturing element:', { selector, filename });
    
    // Use elegant spread pattern with strong types
    const elementParams: StrongScreenshotParams = {
      ...params,
      filename,
      selector,
      fullPage: false
    };
    
    await this.takeScreenshot(elementParams);
  }

  /**
   * Quick snapshot with strong types
   */
  private async quickSnapshot(params: StrongScreenshotParams): Promise<void> {
    const filename = `snap-${Date.now()}.png`;
    
    console.log('📸 Quick snapshot:', filename);
    
    // Use elegant spread pattern with strong types
    const snapParams: StrongScreenshotParams = {
      ...params,
      filename,
      fullPage: true
    };
    
    await this.takeScreenshot(snapParams);
  }
}

// Export for dynamic loading
export default ScreenshotHandler;