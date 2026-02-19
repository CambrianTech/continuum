/**
 * Interface Webmcp Call Command - Browser Implementation
 *
 * Call a WebMCP tool on the current page. Returns structured result from the tool.
 * Fails explicitly if WebMCP is not available or tool not found.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { InterfaceWebmcpCallParams, InterfaceWebmcpCallResult } from '../shared/InterfaceWebmcpCallTypes';
import { createInterfaceWebmcpCallResultFromParams } from '../shared/InterfaceWebmcpCallTypes';
import type { WebMCPToolDefinition } from '../../shared/WebMCPTypes';
import { formDataToObject } from '../../shared/WebMCPTypes';
// Import for side effects (global Navigator declaration)
import '../../shared/WebMCPTypes';

export class InterfaceWebmcpCallBrowserCommand extends CommandBase<InterfaceWebmcpCallParams, InterfaceWebmcpCallResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/webmcp/call', context, subpath, commander);
  }

  async execute(params: InterfaceWebmcpCallParams): Promise<InterfaceWebmcpCallResult> {
    console.log('🌐 BROWSER: Calling WebMCP tool:', params.toolName);

    // Validate required parameters
    if (!params.toolName || params.toolName.trim() === '') {
      throw new ValidationError(
        'toolName',
        'Missing required parameter "toolName". Use interface/webmcp/discover to list available tools.'
      );
    }

    // If URL provided, delegate to server (requires puppeteer)
    if (params.url) {
      console.log('🌐 BROWSER: External URL call requires server-side puppeteer');
      return await this.remoteExecute(params);
    }

    // Check WebMCP availability
    if (typeof navigator === 'undefined' || !navigator.modelContext) {
      return createInterfaceWebmcpCallResultFromParams(params, {
        success: false,
        called: false,
        reason: 'Chrome Canary 146+ with WebMCP flag required. Enable at chrome://flags/#web-mcp-for-testing',
        toolName: params.toolName,
        result: null,
        pageUrl: '',
      });
    }

    // Find and call the tool
    try {
      const result = await this.callTool(params.toolName, params.params);
      return createInterfaceWebmcpCallResultFromParams(params, {
        success: true,
        called: true,
        reason: '',
        toolName: params.toolName,
        result,
        pageUrl: typeof window !== 'undefined' ? window.location.href : '',
      });
    } catch (error) {
      return createInterfaceWebmcpCallResultFromParams(params, {
        success: false,
        called: false,
        reason: error instanceof Error ? error.message : String(error),
        toolName: params.toolName,
        result: null,
        pageUrl: typeof window !== 'undefined' ? window.location.href : '',
      });
    }
  }

  /**
   * Find and call a WebMCP tool by name
   */
  private async callTool(toolName: string, toolParams: object): Promise<unknown> {
    // Check for global tool registry
    if (typeof window !== 'undefined' && window.__webmcp_tools) {
      const tool = window.__webmcp_tools.find((t: WebMCPToolDefinition) => t.name === toolName);
      if (tool) {
        return await tool.execute(toolParams);
      }
    }

    // Check if modelContext has getTools (non-standard)
    if (navigator.modelContext?.getTools) {
      const tools = navigator.modelContext.getTools();
      const tool = tools.find((t: WebMCPToolDefinition) => t.name === toolName);
      if (tool) {
        return await tool.execute(toolParams);
      }
    }

    // Check for declarative form-based tools
    const form = document.querySelector(`form[toolname="${toolName}"]`) as HTMLFormElement | null;
    if (form) {
      return await this.callFormTool(form, toolParams);
    }

    throw new Error(`Tool "${toolName}" not found on page. Use interface/webmcp/discover to list available tools.`);
  }

  /**
   * Call a tool defined via declarative form
   */
  private async callFormTool(form: HTMLFormElement, toolParams: object): Promise<unknown> {
    // Fill form fields with params
    const params = toolParams as Record<string, unknown>;
    for (const [name, value] of Object.entries(params)) {
      const field = form.querySelector(`[name="${name}"]`) as HTMLInputElement | null;
      if (field) {
        field.value = String(value);
        field.dispatchEvent(new Event('input', { bubbles: true }));
        field.dispatchEvent(new Event('change', { bubbles: true }));
      }
    }

    // Submit form and capture result
    return new Promise((resolve, reject) => {
      const submitHandler = (event: SubmitEvent) => {
        // Check if respondWith is available (WebMCP extension)
        if ('respondWith' in event && typeof (event as unknown as { respondWith: (p: Promise<unknown>) => void }).respondWith === 'function') {
          // The form will call respondWith with the result
          const originalRespondWith = (event as unknown as { respondWith: (p: Promise<unknown>) => void }).respondWith;
          (event as unknown as { respondWith: (p: Promise<unknown>) => void }).respondWith = (promise: Promise<unknown>) => {
            promise.then(resolve).catch(reject);
            return originalRespondWith.call(event, promise);
          };
        } else {
          // Form doesn't use respondWith, just resolve with submitted data
          event.preventDefault();
          const formData = new FormData(form);
          resolve(formDataToObject(formData));
        }
        form.removeEventListener('submit', submitHandler);
      };

      form.addEventListener('submit', submitHandler);

      // Check for toolautosubmit attribute
      if (form.hasAttribute('toolautosubmit')) {
        form.requestSubmit();
      } else {
        // Find and click submit button
        const submitButton = form.querySelector('[type="submit"]') as HTMLButtonElement | null;
        if (submitButton) {
          submitButton.click();
        } else {
          form.requestSubmit();
        }
      }

      // Timeout after 30 seconds
      setTimeout(() => {
        form.removeEventListener('submit', submitHandler);
        reject(new Error('Form submission timeout'));
      }, 30000);
    });
  }
}
