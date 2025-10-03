/**
 * Ping Command - Browser Implementation
 * 
 * Browser-side ping command with browser-specific environment detection.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import { PingCommand } from '../shared/PingCommand';
import type { PingResult } from '../shared/PingTypes';

export class PingBrowserCommand extends PingCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async getEnvironmentInfo(): Promise<PingResult['environment']> {
    // Rich browser environment information
    // eslint-disable-next-line no-undef
    const nav = typeof navigator !== 'undefined' ? navigator : null;
    // eslint-disable-next-line no-undef
    const screenObj = typeof window !== 'undefined' && window.screen ? window.screen : null;
    // eslint-disable-next-line no-undef
    const location = typeof window !== 'undefined' && window.location ? window.location : null;

    // Extract browser name and version from userAgent
    const userAgent = nav?.userAgent ?? 'Unknown';
    const browserInfo = this.parseBrowserInfo(userAgent);

    return {
      type: 'browser',
      name: browserInfo.name,
      version: browserInfo.version,
      platform: nav?.platform ?? 'Unknown',
      runtime: userAgent,
      language: nav?.language ?? 'Unknown',
      online: nav?.onLine ?? false,
      // eslint-disable-next-line no-undef
      viewport: typeof window !== 'undefined' ? {
        // eslint-disable-next-line no-undef
        width: window.innerWidth ?? 0,
        // eslint-disable-next-line no-undef
        height: window.innerHeight ?? 0
      } : {
        width: 0,
        height: 0
      },
      screen: screenObj ? {
        width: screenObj.width,
        height: screenObj.height,
        colorDepth: screenObj.colorDepth
      } : {
        width: 0,
        height: 0,
        colorDepth: 0
      },
      url: location?.href ?? 'Unknown',
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Parse browser name and version from userAgent string
   */
  private parseBrowserInfo(userAgent: string): { name: string; version: string } {
    // Chrome
    if (userAgent.includes('Chrome/')) {
      const match = userAgent.match(/Chrome\/(\d+)/);
      return { name: 'Chrome', version: match?.[1] ?? 'Unknown' };
    }
    // Firefox
    if (userAgent.includes('Firefox/')) {
      const match = userAgent.match(/Firefox\/(\d+)/);
      return { name: 'Firefox', version: match?.[1] ?? 'Unknown' };
    }
    // Safari (but not Chrome, which also contains Safari)
    if (userAgent.includes('Safari/') && !userAgent.includes('Chrome')) {
      const match = userAgent.match(/Version\/(\d+)/);
      return { name: 'Safari', version: match?.[1] ?? 'Unknown' };
    }
    // Edge
    if (userAgent.includes('Edg/')) {
      const match = userAgent.match(/Edg\/(\d+)/);
      return { name: 'Edge', version: match?.[1] ?? 'Unknown' };
    }
    return { name: 'Unknown', version: 'Unknown' };
  }

  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }
}