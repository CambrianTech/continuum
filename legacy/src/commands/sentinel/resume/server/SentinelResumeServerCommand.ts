/**
 * Sentinel Resume Command - Server Implementation
 *
 * Routes to Rust sentinel module via IPC.
 * Resumes a pipeline from a durable checkpoint.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { SentinelResumeParams, SentinelResumeResult } from '../shared/SentinelResumeTypes';
import { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class SentinelResumeServerCommand extends CommandBase<SentinelResumeParams, SentinelResumeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/resume', context, subpath, commander);
  }

  async execute(params: SentinelResumeParams): Promise<SentinelResumeResult> {
    const rustClient = RustCoreIPCClient.getInstance();

    try {
      const result = await rustClient.sentinelResume(params.handle);
      return transformPayload(params, {
        success: true,
        handle: result.handle,
        status: result.status,
        resumedFromStep: 0,
      });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      return transformPayload(params, {
        success: false,
        handle: params.handle,
        status: 'failed',
        resumedFromStep: 0,
        error: message,
      });
    }
  }
}
