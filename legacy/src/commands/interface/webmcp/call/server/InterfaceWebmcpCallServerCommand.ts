/**
 * Interface Webmcp Call Command - Server Implementation
 *
 * Call a WebMCP tool on external pages using puppeteer + Chrome Canary.
 * Fails explicitly if requirements not met or tool not found.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { InterfaceWebmcpCallParams, InterfaceWebmcpCallResult } from '../shared/InterfaceWebmcpCallTypes';
import { createInterfaceWebmcpCallResultFromParams } from '../shared/InterfaceWebmcpCallTypes';

export class InterfaceWebmcpCallServerCommand extends CommandBase<InterfaceWebmcpCallParams, InterfaceWebmcpCallResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/webmcp/call', context, subpath, commander);
  }

  async execute(params: InterfaceWebmcpCallParams): Promise<InterfaceWebmcpCallResult> {
    console.log('🔧 SERVER: Calling WebMCP tool on external page', params);

    // Validate required parameters
    if (!params.toolName || params.toolName.trim() === '') {
      throw new ValidationError(
        'toolName',
        'Missing required parameter "toolName". Use interface/webmcp/discover to list available tools.'
      );
    }

    // If no URL provided, this should have been handled browser-side
    if (!params.url) {
      return createInterfaceWebmcpCallResultFromParams(params, {
        success: false,
        called: false,
        reason: 'No URL provided. For current page tool calls, run from browser context.',
        toolName: params.toolName,
        result: null,
        pageUrl: '',
      });
    }

    // Check if puppeteer is available
    const puppeteerCheck = await this.checkPuppeteer();
    if (!puppeteerCheck.available) {
      return createInterfaceWebmcpCallResultFromParams(params, {
        success: false,
        called: false,
        reason: puppeteerCheck.reason,
        toolName: params.toolName,
        result: null,
        pageUrl: params.url,
      });
    }

    // Check for Chrome Canary
    const chromeCanaryCheck = await this.checkChromeCanary();
    if (!chromeCanaryCheck.available) {
      return createInterfaceWebmcpCallResultFromParams(params, {
        success: false,
        called: false,
        reason: chromeCanaryCheck.reason,
        toolName: params.toolName,
        result: null,
        pageUrl: params.url,
      });
    }

    // Call tool via puppeteer
    try {
      const result = await this.callToolWithPuppeteer(
        params.url,
        params.toolName,
        params.params,
        chromeCanaryCheck.executablePath
      );
      return createInterfaceWebmcpCallResultFromParams(params, {
        success: true,
        called: true,
        reason: '',
        toolName: params.toolName,
        result,
        pageUrl: params.url,
      });
    } catch (error) {
      return createInterfaceWebmcpCallResultFromParams(params, {
        success: false,
        called: false,
        reason: `Failed to call tool: ${error instanceof Error ? error.message : String(error)}`,
        toolName: params.toolName,
        result: null,
        pageUrl: params.url,
      });
    }
  }

  private async checkPuppeteer(): Promise<{ available: boolean; reason: string }> {
    try {
      require.resolve('puppeteer');
      return { available: true, reason: '' };
    } catch {
      try {
        require.resolve('puppeteer-core');
        return { available: true, reason: '' };
      } catch {
        return {
          available: false,
          reason: 'Puppeteer not installed. Run: npm install puppeteer-core',
        };
      }
    }
  }

  private async checkChromeCanary(): Promise<{ available: boolean; reason: string; executablePath: string }> {
    const { execSync } = await import('child_process');
    const platform = process.platform;

    let canaryPath = '';
    if (platform === 'darwin') {
      canaryPath = '/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary';
    } else if (platform === 'linux') {
      canaryPath = '/usr/bin/google-chrome-unstable';
    } else if (platform === 'win32') {
      canaryPath = 'C:\\Users\\' + process.env.USERNAME + '\\AppData\\Local\\Google\\Chrome SxS\\Application\\chrome.exe';
    }

    try {
      if (platform === 'darwin') {
        execSync(`test -f "${canaryPath}"`);
      } else {
        execSync(`test -f "${canaryPath}" 2>/dev/null || where "${canaryPath}" 2>nul`);
      }
      return { available: true, reason: '', executablePath: canaryPath };
    } catch {
      return {
        available: false,
        reason: `Chrome Canary not found at ${canaryPath}. Install from https://www.google.com/chrome/canary/`,
        executablePath: '',
      };
    }
  }

  private async callToolWithPuppeteer(
    url: string,
    toolName: string,
    toolParams: object,
    executablePath: string
  ): Promise<unknown> {
    // Dynamic import
    let puppeteer: typeof import('puppeteer-core');
    try {
      puppeteer = await import('puppeteer-core');
    } catch {
      puppeteer = await import('puppeteer') as unknown as typeof import('puppeteer-core');
    }

    const browser = await puppeteer.launch({
      executablePath,
      headless: true,
      args: ['--enable-features=WebMCP'],
    });

    try {
      const page = await browser.newPage();
      await page.goto(url, { waitUntil: 'networkidle2' });

      // Call the tool in page context
      const result = await page.evaluate(async (name: string, params: object) => {
        // Check for global tool registry
        const win = window as unknown as { __webmcp_tools?: Array<{ name: string; execute: (p: unknown) => Promise<unknown> }> };
        if (win.__webmcp_tools) {
          const tool = win.__webmcp_tools.find(t => t.name === name);
          if (tool) {
            return await tool.execute(params);
          }
        }

        // Check for declarative form-based tools
        const form = document.querySelector(`form[toolname="${name}"]`) as HTMLFormElement | null;
        if (form) {
          // Fill form fields
          const p = params as Record<string, unknown>;
          for (const [fieldName, value] of Object.entries(p)) {
            const field = form.querySelector(`[name="${fieldName}"]`) as HTMLInputElement | null;
            if (field) {
              field.value = String(value);
              field.dispatchEvent(new Event('input', { bubbles: true }));
              field.dispatchEvent(new Event('change', { bubbles: true }));
            }
          }

          // Submit and wait for result
          return new Promise((resolve, reject) => {
            const submitHandler = (event: Event) => {
              event.preventDefault();
              const formData = new FormData(form);
              // Convert FormData to object
              const result: Record<string, string> = {};
              formData.forEach((value, key) => {
                result[key] = String(value);
              });
              resolve(result);
              form.removeEventListener('submit', submitHandler);
            };
            form.addEventListener('submit', submitHandler);
            form.requestSubmit();
            setTimeout(() => {
              form.removeEventListener('submit', submitHandler);
              reject(new Error('Form submission timeout'));
            }, 30000);
          });
        }

        throw new Error(`Tool "${name}" not found on page`);
      }, toolName, toolParams);

      return result;
    } finally {
      await browser.close();
    }
  }
}
