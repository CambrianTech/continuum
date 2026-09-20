/**
 * Interface Browser Capabilities Command - Server Implementation
 *
 * Check available browser automation capabilities. Returns explicit status for each capability (webmcp, puppeteer, etc). No fallbacks - AIs see exactly what is/isn't available.
 *
 * Server checks: puppeteer, system browser
 * Browser checks: WebMCP (navigator.modelContext) - done browser-side
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfaceBrowserCapabilitiesParams, InterfaceBrowserCapabilitiesResult } from '../shared/InterfaceBrowserCapabilitiesTypes';
import { createInterfaceBrowserCapabilitiesResultFromParams } from '../shared/InterfaceBrowserCapabilitiesTypes';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

export class InterfaceBrowserCapabilitiesServerCommand extends CommandBase<InterfaceBrowserCapabilitiesParams, InterfaceBrowserCapabilitiesResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/browser/capabilities', context, subpath, commander);
  }

  async execute(params: InterfaceBrowserCapabilitiesParams): Promise<InterfaceBrowserCapabilitiesResult> {
    console.log('🔧 SERVER: Checking server-side browser capabilities (puppeteer, system browser)');

    // Check puppeteer availability
    const puppeteerCheck = await this.checkPuppeteer();

    // Check system browser availability
    const systemBrowserCheck = await this.checkSystemBrowser();

    // Form introspection is available when puppeteer is available
    const formIntrospection = puppeteerCheck.available;

    // Generate helpful hint
    const hint = this.generateHint(puppeteerCheck.available, formIntrospection);

    // WebMCP is checked browser-side, return false here (browser will override)
    return createInterfaceBrowserCapabilitiesResultFromParams(params, {
      success: true,
      webmcp: false,  // Will be overridden by browser-side check
      webmcpReason: 'Checked browser-side',
      puppeteer: puppeteerCheck.available,
      puppeteerReason: puppeteerCheck.reason,
      formIntrospection,
      systemBrowser: systemBrowserCheck.available,
      availableBackends: [],  // Browser will compute final list
      hint,
    });
  }

  /**
   * Check if puppeteer is available
   */
  private async checkPuppeteer(): Promise<{ available: boolean; reason: string }> {
    try {
      // Try to require puppeteer
      require.resolve('puppeteer');
      return { available: true, reason: '' };
    } catch {
      // Try puppeteer-core as alternative
      try {
        require.resolve('puppeteer-core');
        return { available: true, reason: '' };
      } catch {
        return {
          available: false,
          reason: 'Puppeteer not installed. Run: npm install puppeteer',
        };
      }
    }
  }

  /**
   * Check if system browser (open command) is available
   */
  private async checkSystemBrowser(): Promise<{ available: boolean; reason: string }> {
    try {
      // Check platform-specific open command
      const command = this.getOpenCommandTest();
      await execAsync(command);
      return { available: true, reason: '' };
    } catch {
      return {
        available: false,
        reason: `System browser command not available on ${process.platform}`,
      };
    }
  }

  /**
   * Get platform-specific command to test browser availability
   */
  private getOpenCommandTest(): string {
    switch (process.platform) {
      case 'darwin':
        return 'which open';
      case 'win32':
        return 'where start';
      case 'linux':
      default:
        return 'which xdg-open';
    }
  }

  /**
   * Generate helpful hint based on available capabilities
   */
  private generateHint(puppeteer: boolean, formIntrospection: boolean): string {
    if (formIntrospection) {
      return 'Form introspection available. Use interface/page/forms to discover forms on any web page, then interface/page/fill and interface/page/submit to interact with them.';
    }
    if (!puppeteer) {
      return 'Puppeteer not available. Install with: npm install puppeteer-core. Form introspection requires puppeteer.';
    }
    return '';
  }
}
