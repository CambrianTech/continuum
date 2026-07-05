/**
 * Runtime Metrics Command - Browser Implementation
 *
 * Query Rust module performance metrics including latency percentiles, command counts, and slow command tracking. Enables AI-driven system analysis and optimization.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { RuntimeMetricsParams, RuntimeMetricsResult } from '../shared/RuntimeMetricsTypes';

export class RuntimeMetricsBrowserCommand extends CommandBase<RuntimeMetricsParams, RuntimeMetricsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('runtime/metrics', context, subpath, commander);
  }

  async execute(params: RuntimeMetricsParams): Promise<RuntimeMetricsResult> {
    console.log('🌐 BROWSER: Delegating Runtime Metrics to server');
    return await this.remoteExecute(params);
  }
}
