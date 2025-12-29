/**
 * Web Fetch Command - Browser Implementation
 *
 * Browser delegates to server for fetching web pages.
 * Forwards browser headers so server looks like the user's browser.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { WebFetchParams, WebFetchResult } from '../shared/WebFetchTypes';

export class WebFetchBrowserCommand extends CommandBase<WebFetchParams, WebFetchResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/web/fetch', context, subpath, commander);
  }

  async execute(params: WebFetchParams): Promise<WebFetchResult> {
    // Capture browser headers to forward to server
    const browserHeaders: Record<string, string> = {
      'User-Agent': navigator.userAgent,
      'Accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8',
      'Accept-Language': navigator.language || 'en-US,en;q=0.9',
      'Sec-Fetch-Dest': 'document',
      'Sec-Fetch-Mode': 'navigate',
      'Sec-Fetch-Site': 'cross-site',
      'Sec-Fetch-User': '?1',
      'Upgrade-Insecure-Requests': '1',
      ...params.headers // Allow caller to override
    };

    // Delegate to server with browser headers
    return this.remoteExecute<WebFetchParams, WebFetchResult>(
      { ...params, headers: browserHeaders },
      'interface/web/fetch',
      'server'
    );
  }
}
