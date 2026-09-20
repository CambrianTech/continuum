/**
 * Interface Webmcp Discover Command - Browser Implementation
 *
 * Discover WebMCP tools available on the current page. Returns structured tool definitions with schemas. Fails explicitly if WebMCP is not available.
 *
 * NOTE: WebMCP is designed for pages to EXPOSE tools to agents, not for querying external pages.
 * To discover tools on external pages, use puppeteer with Chrome DevTools Protocol (CDP).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InterfaceWebmcpDiscoverParams, InterfaceWebmcpDiscoverResult, WebMCPTool } from '../shared/InterfaceWebmcpDiscoverTypes';
import { createInterfaceWebmcpDiscoverResultFromParams } from '../shared/InterfaceWebmcpDiscoverTypes';
import type { WebMCPToolDefinition } from '../../shared/WebMCPTypes';
// Import for side effects (global Navigator declaration)
import '../../shared/WebMCPTypes';

export class InterfaceWebmcpDiscoverBrowserCommand extends CommandBase<InterfaceWebmcpDiscoverParams, InterfaceWebmcpDiscoverResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/webmcp/discover', context, subpath, commander);
  }

  async execute(params: InterfaceWebmcpDiscoverParams): Promise<InterfaceWebmcpDiscoverResult> {
    console.log('🌐 BROWSER: Discovering WebMCP tools');

    // Check WebMCP availability
    if (typeof navigator === 'undefined' || !navigator.modelContext) {
      return createInterfaceWebmcpDiscoverResultFromParams(params, {
        success: false,
        available: false,
        reason: 'Chrome Canary 146+ with WebMCP flag required. Enable at chrome://flags/#web-mcp-for-testing',
        tools: [],
        pageUrl: '',
      });
    }

    // If URL provided, we need to navigate there first
    // This requires puppeteer/CDP for external pages - delegate to server
    if (params.url) {
      console.log('🌐 BROWSER: External URL discovery requires server-side puppeteer');
      return await this.remoteExecute(params);
    }

    // Try to get tools from current page context
    const tools = this.discoverCurrentPageTools();

    return createInterfaceWebmcpDiscoverResultFromParams(params, {
      success: true,
      available: true,
      reason: '',
      tools,
      pageUrl: typeof window !== 'undefined' ? window.location.href : '',
    });
  }

  /**
   * Discover tools registered on the current page
   *
   * NOTE: The WebMCP spec doesn't define a standard way to query tools.
   * Tools are exposed BY the page, consumed BY the agent. This method
   * checks if our page has registered any tools we can report.
   */
  private discoverCurrentPageTools(): WebMCPTool[] {
    // Check if there's a non-standard getTools() method
    if (navigator.modelContext?.getTools) {
      try {
        const rawTools = navigator.modelContext.getTools();
        return rawTools.map((t: WebMCPToolDefinition) => ({
          name: t.name,
          description: t.description,
          inputSchema: t.inputSchema,
        }));
      } catch (e) {
        console.warn('WebMCP getTools() failed:', e);
      }
    }

    // Check for any global tool registry we might have set up
    if (typeof window !== 'undefined' && window.__webmcp_tools) {
      return window.__webmcp_tools.map((t: WebMCPToolDefinition) => ({
        name: t.name,
        description: t.description,
        inputSchema: t.inputSchema,
      }));
    }

    // No tools discovered on current page
    return [];
  }
}
