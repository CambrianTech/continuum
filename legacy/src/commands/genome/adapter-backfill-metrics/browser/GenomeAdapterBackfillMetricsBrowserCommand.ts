/**
 * Genome Adapter Backfill Metrics Command - Browser Implementation
 *
 * Delegates to server (filesystem operation).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeAdapterBackfillMetricsParams, GenomeAdapterBackfillMetricsResult } from '../shared/GenomeAdapterBackfillMetricsTypes';

export class GenomeAdapterBackfillMetricsBrowserCommand extends CommandBase<GenomeAdapterBackfillMetricsParams, GenomeAdapterBackfillMetricsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/adapter-backfill-metrics', context, subpath, commander);
  }

  async execute(params: GenomeAdapterBackfillMetricsParams): Promise<GenomeAdapterBackfillMetricsResult> {
    return await this.remoteExecute(params);
  }
}
