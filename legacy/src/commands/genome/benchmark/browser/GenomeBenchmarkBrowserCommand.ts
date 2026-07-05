/**
 * Genome Benchmark Command - Browser Implementation
 *
 * Run standard and Continuum-specific benchmarks against a model or adapter. Stores results in BenchmarkResultEntity and embeds in adapter manifest for model card publishing. Supports HumanEval, MBPP, RealClassEval, and collaborative team benchmarks.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeBenchmarkParams, GenomeBenchmarkResult } from '../shared/GenomeBenchmarkTypes';

export class GenomeBenchmarkBrowserCommand extends CommandBase<GenomeBenchmarkParams, GenomeBenchmarkResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/benchmark', context, subpath, commander);
  }

  async execute(params: GenomeBenchmarkParams): Promise<GenomeBenchmarkResult> {
    console.log('🌐 BROWSER: Delegating Genome Benchmark to server');
    return await this.remoteExecute(params);
  }
}
