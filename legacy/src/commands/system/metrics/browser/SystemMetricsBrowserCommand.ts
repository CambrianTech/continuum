/**
 * System Metrics Command - Browser Implementation
 *
 * Delegates to server where MetricsCollector queries the dedicated metrics database.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SystemMetricsParams, SystemMetricsResult } from '../shared/SystemMetricsTypes';

export class SystemMetricsBrowserCommand extends CommandBase<SystemMetricsParams, SystemMetricsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('system/metrics', context, subpath, commander);
  }

  async execute(params: SystemMetricsParams): Promise<SystemMetricsResult> {
    return await this.remoteExecute(params);
  }
}
