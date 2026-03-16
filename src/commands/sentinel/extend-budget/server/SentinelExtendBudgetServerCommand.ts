/**
 * Sentinel Extend Budget Command - Server Implementation
 *
 * Routes to Rust sentinel module via IPC.
 * Extends budget limits for a running or paused pipeline.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { SentinelExtendBudgetParams, SentinelExtendBudgetResult } from '../shared/SentinelExtendBudgetTypes';
import type { BudgetLimits } from '../../../../shared/generated/sentinel/BudgetLimits';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SentinelExtendBudgetServerCommand extends CommandBase<SentinelExtendBudgetParams, SentinelExtendBudgetResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/extend-budget', context, subpath, commander);
  }

  async execute(params: SentinelExtendBudgetParams): Promise<SentinelExtendBudgetResult> {
    const rustClient = RustCoreIPCClient.getInstance();

    try {
      const result = await rustClient.sentinelExtendBudget(params.handle, {
        maxTimeSecs: params.maxTimeSecs,
        maxCostUsd: params.maxCostUsd,
        maxTokens: params.maxTokens,
        maxIterations: params.maxIterations,
      });
      return transformPayload(params, {
        success: true,
        handle: result.handle,
        budgetLimits: result.budgetLimits,
      });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      return transformPayload(params, {
        success: false,
        handle: params.handle,
        budgetLimits: {} as BudgetLimits,
        error: message,
      });
    }
  }
}
