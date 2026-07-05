/**
 * Interface Browser Capabilities Command - Browser Implementation
 *
 * Check available browser automation capabilities. Returns explicit status for each capability (webmcp, puppeteer, etc). No fallbacks - AIs see exactly what is/isn't available.
 *
 * WebMCP detection MUST happen in browser - navigator.modelContext is a browser API.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfaceBrowserCapabilitiesParams, InterfaceBrowserCapabilitiesResult } from '../shared/InterfaceBrowserCapabilitiesTypes';
import { createInterfaceBrowserCapabilitiesResultFromParams } from '../shared/InterfaceBrowserCapabilitiesTypes';
import { checkWebMCPAvailability } from '../../../webmcp/shared/WebMCPTypes';

export class InterfaceBrowserCapabilitiesBrowserCommand extends CommandBase<InterfaceBrowserCapabilitiesParams, InterfaceBrowserCapabilitiesResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/browser/capabilities', context, subpath, commander);
  }

  async execute(params: InterfaceBrowserCapabilitiesParams): Promise<InterfaceBrowserCapabilitiesResult> {
    console.log('🌐 BROWSER: Checking browser capabilities (WebMCP, etc)');

    // Check WebMCP availability (browser-side API)
    const webmcpCheck = this.checkWebMCP();

    // Get server-side capabilities (puppeteer, system browser)
    const serverCaps = await this.remoteExecute(params) as InterfaceBrowserCapabilitiesResult;

    // Merge browser + server capabilities
    const availableBackends: string[] = [];
    if (webmcpCheck.available) availableBackends.push('webmcp');
    if (serverCaps.puppeteer) availableBackends.push('puppeteer');
    if (serverCaps.formIntrospection) availableBackends.push('form-introspection');
    if (serverCaps.systemBrowser) availableBackends.push('system-browser');

    // Generate merged hint
    const hint = this.generateHint(webmcpCheck.available, serverCaps.formIntrospection);

    return createInterfaceBrowserCapabilitiesResultFromParams(params, {
      success: true,
      webmcp: webmcpCheck.available,
      webmcpReason: webmcpCheck.reason,
      puppeteer: serverCaps.puppeteer,
      puppeteerReason: serverCaps.puppeteerReason,
      formIntrospection: serverCaps.formIntrospection,
      systemBrowser: serverCaps.systemBrowser,
      availableBackends,
      hint,
    });
  }

  /**
   * Check if WebMCP (navigator.modelContext) is available
   */
  private checkWebMCP(): { available: boolean; reason: string } {
    return checkWebMCPAvailability();
  }

  /**
   * Generate helpful hint based on available capabilities
   */
  private generateHint(webmcp: boolean, formIntrospection: boolean): string {
    if (webmcp) {
      return 'WebMCP available! Use interface/webmcp/discover to find tools. Form introspection also available via interface/page/forms.';
    }
    if (formIntrospection) {
      return 'Form introspection available. Use interface/page/forms to discover forms on any web page, then interface/page/fill and interface/page/submit to interact with them.';
    }
    return 'No browser automation capabilities available.';
  }
}
