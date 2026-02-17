/**
 * Indicator Command - Visual feedback for real-time events
 * Creates browser indicators to demonstrate server→browser event routing
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload, CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';

export interface IndicatorParams extends CommandParams {
  message: string;
  title?: string;
  type?: 'info' | 'success' | 'warning' | 'error';
}

export interface IndicatorResult extends CommandResult {
  alerted: boolean;
  message: string;
  timestamp: string;
}

export abstract class IndicatorCommand extends CommandBase<IndicatorParams, IndicatorResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('indicator', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<IndicatorResult> {
    const indicatorParams = params as IndicatorParams;

    if (!indicatorParams?.message) {
      return {
        context: indicatorParams.context,
        sessionId: indicatorParams.sessionId,
        alerted: false,
        message: 'Invalid parameters',
        timestamp: new Date().toISOString()
      };
    }

    const result = await this.showIndicator(indicatorParams);

    // Ensure result has required CommandResult fields
    return {
      ...result,
      context: indicatorParams.context,
      sessionId: indicatorParams.sessionId
    };
  }

  protected abstract showIndicator(params: IndicatorParams): Promise<IndicatorResult>;
}