/**
 * Server Indicator Command - Logs indicators for server-side operations
 */

import { IndicatorCommand, type IndicatorParams, type IndicatorResult } from '../shared/IndicatorCommand';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';

export class IndicatorServerCommand extends IndicatorCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async showIndicator(params: IndicatorParams): Promise<IndicatorResult> {
    const title = params.title ? `[${params.title}] ` : '';
    const typeIcon = this.getTypeIcon(params.type || 'info');

    console.log(`🔔 ${typeIcon} SERVER-INDICATOR-${Date.now()}: ${title}${params.message} - delegating to browser`);

    // Delegate to browser for visual indicator creation
    return await this.remoteExecute(params);
  }

  private getTypeIcon(type: string): string {
    const icons = {
      'info': 'ℹ️',
      'success': '✅',
      'warning': '⚠️',
      'error': '❌'
    };
    return icons[type as keyof typeof icons] || icons.info;
  }
}