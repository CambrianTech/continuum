/**
 * Genome Adapter Update Metrics Command - Browser Implementation
 *
 * Delegates to server (filesystem operation).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAdapterUpdateMetricsParams, GenomeAdapterUpdateMetricsResult } from '../shared/GenomeAdapterUpdateMetricsTypes';

export class GenomeAdapterUpdateMetricsBrowserCommand extends CommandBase<GenomeAdapterUpdateMetricsParams, GenomeAdapterUpdateMetricsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-update-metrics', context, subpath, commander);
  }

  async execute(params: GenomeAdapterUpdateMetricsParams): Promise<GenomeAdapterUpdateMetricsResult> {
    return await this.remoteExecute(params);
  }
}
