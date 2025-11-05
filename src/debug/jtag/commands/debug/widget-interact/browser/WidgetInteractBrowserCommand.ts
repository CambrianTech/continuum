/**
 * Widget Interaction Browser Command
 *
 * Complete widget control - everything you can do in UX/devtools
 * Perfect for MCP integration and screenshot-driven development
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { WidgetInteractParams, WidgetInteractResult } from '../shared/WidgetInteractTypes';
import { createWidgetInteractResult } from '../shared/WidgetInteractTypes';
import { WidgetDiscovery } from '../../../../system/core/browser/utils/WidgetIntrospection';

export class WidgetInteractBrowserCommand extends CommandBase<WidgetInteractParams, WidgetInteractResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('widget-interact', context, subpath, commander);
  }

  async execute(params: WidgetInteractParams): Promise<WidgetInteractResult> {
    const logs: string[] = [];
    const warnings: string[] = [];
    const errors: string[] = [];

    try {
      logs.push(`🎯 Widget interaction: ${params.action} on ${params.widgetSelector}`);

      // Find the target widget
      const widgetRef = WidgetDiscovery.findWidget(params.widgetSelector);

      if (!widgetRef) {
        return createWidgetInteractResult(this.context, this.context.uuid, {
          success: false,
          action: params.action,
          widgetFound: false,
          widgetPath: `${params.widgetSelector} not found`,
          error: `Widget ${params.widgetSelector} not found`,
          debugging: { logs, warnings, errors }
        });
      }

      logs.push(`✅ Widget found: ${widgetRef.path}`);

      // Take screenshot before action if requested
      let screenshotBefore: string | undefined;
      if (params.screenshotBefore) {
        screenshotBefore = await this.takeScreenshot('before', params.screenshotFilename, logs);
      }

      // Execute the requested action
      const actionResult = await this.executeAction(params, widgetRef.element, logs, warnings, errors);

      // Take screenshot after action if requested
      let screenshotAfter: string | undefined;
      if (params.screenshotAfter) {
        screenshotAfter = await this.takeScreenshot('after', params.screenshotFilename, logs);
      }

      // Verify result if requested
      let verification;
      if (params.verifyResult) {
        verification = await this.verifyAction(params, widgetRef.element, logs);
      }

      return createWidgetInteractResult(this.context, this.context.uuid, {
        success: true,
        action: params.action,
        widgetFound: true,
        widgetPath: widgetRef.path,
        actionResult,
        screenshots: {
          before: screenshotBefore,
          after: screenshotAfter
        },
        verification,
        debugging: { logs, warnings, errors }
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      errors.push(`Widget interaction failed: ${errorMessage}`);

      return createWidgetInteractResult(this.context, this.context.uuid, {
        success: false,
        action: params.action,
        error: errorMessage,
        debugging: { logs, warnings, errors }
      });
    }
  }

  private async executeAction(
    params: WidgetInteractParams,
    widget: HTMLElement,
    logs: string[],
    warnings: string[],
    errors: string[]
  ) {
    try {
      switch (params.action) {
        case 'sendMessage':
          return await this.sendChatMessage(widget, params.text || '', logs);

        case 'click':
          return await this.clickElement(widget, params.elementSelector || '', logs);

        case 'type':
          return await this.typeInElement(widget, params.elementSelector || '', params.text || '', logs);

        case 'callMethod':
          return await this.callWidgetMethod(widget, params.methodName || '', params.methodArgs, logs);

        case 'setProperty':
          return await this.setWidgetProperty(widget, params.propertyName || '', params.propertyValue, logs);

        case 'selectRoom':
          return await this.selectRoom(widget, params.text || '', logs);

        default:
          throw new Error(`Unknown action: ${params.action}`);
      }
    } catch (error) {
      errors.push(`Action execution failed: ${error}`);
      return { executed: false, error: String(error) };
    }
  }

  private async sendChatMessage(widget: HTMLElement, message: string, logs: string[]) {
    logs.push(`💬 Sending chat message: "${message}"`);

    const shadowRoot = widget.shadowRoot;
    if (!shadowRoot) throw new Error('No shadow root found');

    // Find input and send button
    const input = shadowRoot.querySelector('.message-input, #messageInput') as HTMLInputElement;
    const sendButton = shadowRoot.querySelector('.send-button, #sendButton') as HTMLButtonElement;

    if (!input) throw new Error('Message input not found');
    if (!sendButton) throw new Error('Send button not found');

    // Type message and click send
    input.value = message;
    input.dispatchEvent(new Event('input', { bubbles: true }));

    sendButton.click();

    logs.push(`✅ Message sent via UI interaction`);
    return { executed: true, returnValue: `Sent: "${message}"` };
  }

  private async clickElement(widget: HTMLElement, selector: string, logs: string[]) {
    logs.push(`🖱️ Clicking element: ${selector}`);

    const shadowRoot = widget.shadowRoot;
    if (!shadowRoot) throw new Error('No shadow root found');

    const element = shadowRoot.querySelector(selector) as HTMLElement;
    if (!element) throw new Error(`Element not found: ${selector}`);

    element.click();
    logs.push(`✅ Clicked: ${selector}`);
    return { executed: true, returnValue: `Clicked ${selector}` };
  }

  private async typeInElement(widget: HTMLElement, selector: string, text: string, logs: string[]) {
    logs.push(`⌨️ Typing in element: ${selector} = "${text}"`);

    const shadowRoot = widget.shadowRoot;
    if (!shadowRoot) throw new Error('No shadow root found');

    const element = shadowRoot.querySelector(selector) as HTMLInputElement;
    if (!element) throw new Error(`Element not found: ${selector}`);

    element.value = text;
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));

    logs.push(`✅ Typed "${text}" in ${selector}`);
    return { executed: true, returnValue: `Typed: "${text}"` };
  }

  private async callWidgetMethod(widget: HTMLElement, methodName: string, args: any[] = [], logs: string[]) {
    logs.push(`🔧 Calling widget method: ${methodName}(${args.map(a => JSON.stringify(a)).join(', ')})`);

    const widgetObj = widget as any;
    if (typeof widgetObj[methodName] !== 'function') {
      throw new Error(`Method ${methodName} not found on widget`);
    }

    const result = widgetObj[methodName].apply(widgetObj, args);
    logs.push(`✅ Method called: ${methodName} returned ${typeof result}`);
    return { executed: true, returnValue: result };
  }

  private async setWidgetProperty(widget: HTMLElement, propertyName: string, value: any, logs: string[]) {
    logs.push(`📝 Setting widget property: ${propertyName} = ${JSON.stringify(value)}`);

    const widgetObj = widget as any;
    widgetObj[propertyName] = value;

    logs.push(`✅ Property set: ${propertyName}`);
    return { executed: true, returnValue: `Set ${propertyName} to ${JSON.stringify(value)}` };
  }

  private async selectRoom(widget: HTMLElement, roomName: string, logs: string[]) {
    logs.push(`🏠 Selecting room: "${roomName}"`);

    const shadowRoot = widget.shadowRoot;
    if (!shadowRoot) throw new Error('No shadow root found');

    // Find room item by text content
    const roomItems = shadowRoot.querySelectorAll('.room-item');
    const roomItem = Array.from(roomItems).find(item =>
      item.textContent?.includes(roomName)
    ) as HTMLElement;

    if (!roomItem) throw new Error(`Room not found: ${roomName}`);

    roomItem.click();
    logs.push(`✅ Selected room: ${roomName}`);
    return { executed: true, returnValue: `Selected room: ${roomName}` };
  }

  private async takeScreenshot(timing: 'before' | 'after', customFilename?: string, logs?: string[]): Promise<string> {
    try {
      const timestamp = Date.now();
      const filename = customFilename
        ? `${customFilename}-${timing}-${timestamp}.png`
        : `widget-interact-${timing}-${timestamp}.png`;

      // Use existing screenshot command via remote execution
      const screenshotResult = await this.remoteExecute({
        context: this.context,
        sessionId: this.context.uuid,
        querySelector: 'body',
        filename
      }, 'screenshot', this.context.environment) as any;

      if (screenshotResult?.success) {
        logs?.push(`📸 Screenshot taken: ${filename}`);
        return screenshotResult.filepath || filename;
      } else {
        logs?.push(`⚠️ Screenshot failed: ${screenshotResult?.error || 'Unknown error'}`);
        return '';
      }
    } catch (error) {
      logs?.push(`⚠️ Screenshot error: ${error}`);
      return '';
    }
  }

  private async verifyAction(params: WidgetInteractParams, widget: HTMLElement, logs: string[]) {
    logs.push(`🔍 Verifying action result...`);

    // Simple verification - could be enhanced based on expectedChange
    let success = true;
    let details = 'Action appears successful';

    if (params.action === 'sendMessage') {
      // Check if input was cleared after sending
      const shadowRoot = widget.shadowRoot;
      const input = shadowRoot?.querySelector('.message-input, #messageInput') as HTMLInputElement;
      success = !input || input.value === '';
      details = success ? 'Message input cleared after sending' : 'Message input still has content';
    }

    logs.push(`${success ? '✅' : '❌'} Verification: ${details}`);
    return { performed: true, success, details };
  }
}