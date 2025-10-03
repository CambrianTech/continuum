/**
 * Ping Command - Browser Implementation
 *
 * Browser collects browser info, then delegates to server for server info.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { PingParams, PingResult, BrowserEnvironmentInfo } from '../shared/PingTypes';
import { VERSION, PACKAGE_NAME } from '../../../shared/version';

export class PingBrowserCommand extends CommandBase<PingParams, PingResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ping', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<PingResult> {
    const pingParams = params as PingParams;
    const browser = this.getBrowserInfo();

    // If server already collected its info, we're the final step
    if (pingParams.server) {
      return {
        ...pingParams,
        success: true,
        browser,
        timestamp: new Date().toISOString()
      };
    }

    // No server info yet - delegate to server to collect it
    return await this.remoteExecute({ ...pingParams, browser });
  }

  private getBrowserInfo(): BrowserEnvironmentInfo {
    /* eslint-disable no-undef */
    const ua = navigator.userAgent;
    const browserInfo = this.parseBrowser(ua);

    return {
      type: 'browser',
      packageName: PACKAGE_NAME,
      packageVersion: VERSION,
      name: browserInfo.name,
      version: browserInfo.version,
      platform: navigator.platform,
      runtime: ua,
      language: navigator.language,
      online: navigator.onLine,
      viewport: {
        width: window.innerWidth,
        height: window.innerHeight
      },
      screen: {
        width: window.screen.width,
        height: window.screen.height,
        colorDepth: window.screen.colorDepth
      },
      url: window.location.href,
      timestamp: new Date().toISOString()
    };
    /* eslint-enable no-undef */
  }

  private parseBrowser(ua: string): { name: string; version: string } {
    if (ua.includes('Chrome/')) {
      const m = ua.match(/Chrome\/(\d+)/);
      return { name: 'Chrome', version: m?.[1] ?? 'Unknown' };
    }
    if (ua.includes('Firefox/')) {
      const m = ua.match(/Firefox\/(\d+)/);
      return { name: 'Firefox', version: m?.[1] ?? 'Unknown' };
    }
    if (ua.includes('Safari/') && !ua.includes('Chrome')) {
      const m = ua.match(/Version\/(\d+)/);
      return { name: 'Safari', version: m?.[1] ?? 'Unknown' };
    }
    if (ua.includes('Edg/')) {
      const m = ua.match(/Edg\/(\d+)/);
      return { name: 'Edge', version: m?.[1] ?? 'Unknown' };
    }
    return { name: 'Unknown', version: 'Unknown' };
  }
}
