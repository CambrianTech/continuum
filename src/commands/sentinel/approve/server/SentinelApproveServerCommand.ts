/**
 * Sentinel Approve Command - Server Implementation
 *
 * Routes to Rust sentinel module via IPC.
 * Approves or rejects a pending pipeline approval step.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { SentinelApproveParams, SentinelApproveResult } from '../shared/SentinelApproveTypes';
import { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class SentinelApproveServerCommand extends CommandBase<SentinelApproveParams, SentinelApproveResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/approve', context, subpath, commander);
  }

  async execute(params: SentinelApproveParams): Promise<SentinelApproveResult> {
    const rustClient = RustCoreIPCClient.getInstance();

    try {
      const result = await rustClient.sentinelApprove(
        params.handle,
        params.approved,
        params.reason,
        params.approverId,
      );
      return transformPayload(params, {
        success: true,
        handle: result.handle,
        approved: result.approved,
      });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      return transformPayload(params, {
        success: false,
        handle: params.handle,
        approved: false,
        error: message,
      });
    }
  }
}
