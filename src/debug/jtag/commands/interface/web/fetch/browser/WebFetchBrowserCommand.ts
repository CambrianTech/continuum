/**
 * Web Fetch Command - Browser Implementation
 *
 * Browser delegates to server for fetching web pages.
 * Server handles CORS and HTML processing.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { WebFetchParams, WebFetchResult } from '../shared/WebFetchTypes';

export class WebFetchBrowserCommand extends CommandBase<WebFetchParams, WebFetchResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/web/fetch', context, subpath, commander);
  }

  async execute(params: WebFetchParams): Promise<WebFetchResult> {
    // Delegate to server - it can fetch cross-origin URLs without CORS issues
    return this.remoteExecute<WebFetchParams, WebFetchResult>(params, 'interface/web/fetch', 'server');
  }
}
