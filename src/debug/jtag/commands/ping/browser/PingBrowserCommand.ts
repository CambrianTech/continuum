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
    const platformInfo = this.getPlatformInfo(ua);

    return {
      type: 'browser',
      packageName: PACKAGE_NAME,
      packageVersion: VERSION,
      name: browserInfo.name,
      version: browserInfo.version,
      platform: platformInfo,
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

  private getPlatformInfo(ua: string): string {
    // Try to detect actual architecture from user agent
    // Note: navigator.platform is deprecated and always returns "MacIntel" for compatibility
    if (ua.includes('Mac OS X')) {
      // Check for ARM indicators in UA string (though browsers hide this)
      // Fall back to navigator.platform but note it's not reliable for M1
      return 'macOS'; // Generic since browsers don't expose ARM vs Intel reliably
    }
    if (ua.includes('Windows')) {
      return 'Windows';
    }
    if (ua.includes('Linux')) {
      return 'Linux';
    }
    if (ua.includes('Android')) {
      return 'Android';
    }
    if (ua.includes('iOS') || ua.includes('iPhone') || ua.includes('iPad')) {
      return 'iOS';
    }
    /* eslint-disable no-undef */
    return navigator.platform; // Fallback to deprecated API
    /* eslint-enable no-undef */
  }

  private parseBrowser(ua: string): { name: string; version: string } {
    // Check Opera first (uses Chrome engine but has OPR in UA)
    if (ua.includes('OPR/') || ua.includes('Opera/')) {
      const m = ua.match(/(?:OPR|Opera)\/(\d+)/);
      return { name: 'Opera', version: m?.[1] ?? 'Unknown' };
    }
    // Check Edge before Chrome (also uses Chromium)
    if (ua.includes('Edg/')) {
      const m = ua.match(/Edg\/(\d+)/);
      return { name: 'Edge', version: m?.[1] ?? 'Unknown' };
    }
    // Check Chrome
    if (ua.includes('Chrome/')) {
      const m = ua.match(/Chrome\/(\d+)/);
      return { name: 'Chrome', version: m?.[1] ?? 'Unknown' };
    }
    // Check Firefox
    if (ua.includes('Firefox/')) {
      const m = ua.match(/Firefox\/(\d+)/);
      return { name: 'Firefox', version: m?.[1] ?? 'Unknown' };
    }
    // Check Safari (no Chrome in UA for real Safari)
    if (ua.includes('Safari/') && !ua.includes('Chrome')) {
      const m = ua.match(/Version\/(\d+)/);
      return { name: 'Safari', version: m?.[1] ?? 'Unknown' };
    }
    return { name: 'Unknown', version: 'Unknown' };
  }
}
