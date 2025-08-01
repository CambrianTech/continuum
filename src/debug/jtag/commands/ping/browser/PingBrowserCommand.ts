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
    const nav = typeof navigator !== 'undefined' ? navigator : null;
    const screen = typeof window !== 'undefined' && window.screen ? window.screen : null;
    const location = typeof window !== 'undefined' && window.location ? window.location : null;
    
    return {
      type: 'browser',
      userAgent: nav?.userAgent || 'Unknown',
      platform: nav?.platform || 'Unknown',
      language: nav?.language || 'Unknown',
      cookieEnabled: nav?.cookieEnabled ?? false,
      onLine: nav?.onLine ?? false,
      screenResolution: screen ? {
        width: screen.width,
        height: screen.height,
        colorDepth: screen.colorDepth
      } : {
        width: 0,
        height: 0,
        colorDepth: 0
      },
      viewport: typeof window !== 'undefined' ? {
        width: window.innerWidth || 0,
        height: window.innerHeight || 0
      } : {
        width: 0,
        height: 0
      },
      url: location?.href || 'Unknown',
      timestamp: new Date().toISOString()
    };
  }

  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }
}