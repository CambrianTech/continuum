/**
 * Sentinel Status Command - Server Implementation
 *
 * Check status of a running sentinel by handle.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelStatusParams, SentinelStatusResult } from '../shared/SentinelStatusTypes';
import { getSentinelStatus } from '../../run/server/SentinelRunServerCommand';

export class SentinelStatusServerCommand extends CommandBase<SentinelStatusParams, SentinelStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/status', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelStatusResult> {
    const statusParams = params as SentinelStatusParams;

    if (!statusParams.handle) {
      return transformPayload(params, {
        success: false,
        handle: '',
        status: 'not_found',
        error: 'Handle is required',
      });
    }

    const handle = getSentinelStatus(statusParams.handle);

    if (!handle) {
      return transformPayload(params, {
        success: false,
        handle: statusParams.handle,
        status: 'not_found',
        error: 'Handle not found',
      });
    }

    return transformPayload(params, {
      success: true,
      handle: handle.id,
      type: handle.type,
      status: handle.status,
      progress: handle.progress,
      duration: Date.now() - handle.startTime,
      data: handle.data,
      error: handle.error,
    });
  }
}
