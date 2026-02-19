/**
 * Interface Webmcp Discover Command - Server Implementation
 *
 * Discover WebMCP tools on external pages using puppeteer + Chrome DevTools Protocol.
 * Fails explicitly if requirements not met.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfaceWebmcpDiscoverParams, InterfaceWebmcpDiscoverResult, WebMCPTool } from '../shared/InterfaceWebmcpDiscoverTypes';
import { createInterfaceWebmcpDiscoverResultFromParams } from '../shared/InterfaceWebmcpDiscoverTypes';

export class InterfaceWebmcpDiscoverServerCommand extends CommandBase<InterfaceWebmcpDiscoverParams, InterfaceWebmcpDiscoverResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/webmcp/discover', context, subpath, commander);
  }

  async execute(params: InterfaceWebmcpDiscoverParams): Promise<InterfaceWebmcpDiscoverResult> {
    console.log('🔧 SERVER: Discovering WebMCP tools on external page', params);

    // If no URL provided, this should have been handled browser-side
    if (!params.url) {
      return createInterfaceWebmcpDiscoverResultFromParams(params, {
        success: false,
        available: false,
        reason: 'No URL provided. For current page discovery, run from browser context.',
        tools: [],
        pageUrl: '',
      });
    }

    // Check if puppeteer is available
    const puppeteerCheck = await this.checkPuppeteer();
    if (!puppeteerCheck.available) {
      return createInterfaceWebmcpDiscoverResultFromParams(params, {
        success: false,
        available: false,
        reason: puppeteerCheck.reason,
        tools: [],
        pageUrl: params.url,
      });
    }

    // Check for Chrome Canary with WebMCP
    const chromeCanaryCheck = await this.checkChromeCanary();
    if (!chromeCanaryCheck.available) {
      return createInterfaceWebmcpDiscoverResultFromParams(params, {
        success: false,
        available: false,
        reason: chromeCanaryCheck.reason,
        tools: [],
        pageUrl: params.url,
      });
    }

    // Launch puppeteer with Chrome Canary and discover tools
    try {
      const tools = await this.discoverToolsWithPuppeteer(params.url, chromeCanaryCheck.executablePath);
      return createInterfaceWebmcpDiscoverResultFromParams(params, {
        success: true,
        available: true,
        reason: '',
        tools,
        pageUrl: params.url,
      });
    } catch (error) {
      return createInterfaceWebmcpDiscoverResultFromParams(params, {
        success: false,
        available: false,
        reason: `Failed to discover tools: ${error instanceof Error ? error.message : String(error)}`,
        tools: [],
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
      // Check if Chrome Canary exists
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

  private async discoverToolsWithPuppeteer(url: string, executablePath: string): Promise<WebMCPTool[]> {
    // Dynamic import to handle case when puppeteer is not installed
    let puppeteer: typeof import('puppeteer-core');
    try {
      puppeteer = await import('puppeteer-core');
    } catch {
      puppeteer = await import('puppeteer') as unknown as typeof import('puppeteer-core');
    }

    const browser = await puppeteer.launch({
      executablePath,
      headless: true,
      args: [
        '--enable-features=WebMCP',  // Enable WebMCP flag
      ],
    });

    try {
      const page = await browser.newPage();
      await page.goto(url, { waitUntil: 'networkidle2' });

      // Query tools registered via WebMCP
      // Note: This requires the page to have called navigator.modelContext.provideContext()
      const tools = await page.evaluate(() => {
        // Check for global tool registry the page might expose
        const win = window as unknown as { __webmcp_tools?: Array<{ name: string; description: string; inputSchema: Record<string, unknown> }> };
        if (win.__webmcp_tools) {
          return win.__webmcp_tools;
        }

        // Check if modelContext exists and has a getTools method (non-standard)
        const nav = navigator as unknown as { modelContext?: { getTools?: () => Array<{ name: string; description: string; inputSchema: Record<string, unknown> }> } };
        if (nav.modelContext?.getTools) {
          return nav.modelContext.getTools();
        }

        // Check for declarative form-based tools
        const forms = document.querySelectorAll('form[toolname]');
        const formTools: Array<{ name: string; description: string; inputSchema: Record<string, unknown> }> = [];
        forms.forEach(form => {
          const toolname = form.getAttribute('toolname');
          const tooldescription = form.getAttribute('tooldescription') || '';
          if (toolname) {
            // Build schema from form fields
            const inputSchema: Record<string, unknown> = { type: 'object', properties: {} };
            const props = inputSchema.properties as Record<string, unknown>;
            form.querySelectorAll('input, select, textarea').forEach(field => {
              const name = (field as HTMLInputElement).name;
              if (name) {
                props[name] = { type: 'string' };
              }
            });
            formTools.push({ name: toolname, description: tooldescription, inputSchema });
          }
        });
        return formTools;
      });

      return tools || [];
    } finally {
      await browser.close();
    }
  }
}
