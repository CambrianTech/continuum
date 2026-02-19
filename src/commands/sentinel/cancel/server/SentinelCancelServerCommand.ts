/**
 * Sentinel Cancel Command - Server Implementation
 *
 * Cancels running sentinels by handle or filter.
 * Uses sentinelList() to find matching handles, then sentinelCancel() on each.
 *
 * Default behavior (no params): cancels all running sentinels.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelCancelParams, SentinelCancelResult, CancelledSentinel } from '../shared/SentinelCancelTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { SentinelHandle } from '../../../../workers/continuum-core/bindings/modules/sentinel';

export class SentinelCancelServerCommand extends CommandBase<SentinelCancelParams, SentinelCancelResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/cancel', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelCancelResult> {
    const cancelParams = params as SentinelCancelParams;
    const rustClient = RustCoreIPCClient.getInstance();

    try {
      // Direct cancel: single handle provided
      if (cancelParams.handle) {
        const result = await this.cancelOne(rustClient, cancelParams.handle);
        return transformPayload(params, {
          success: result.cancelled,
          cancelled: [result],
          totalCancelled: result.cancelled ? 1 : 0,
          totalAttempted: 1,
        });
      }

      // Filtered cancel: list handles and filter
      const listResult = await rustClient.sentinelList();
      const handles = this.filterHandles(listResult.handles, cancelParams);

      if (handles.length === 0) {
        return transformPayload(params, {
          success: true,
          cancelled: [],
          totalCancelled: 0,
          totalAttempted: 0,
        });
      }

      // Cancel all matching handles
      const results: CancelledSentinel[] = [];
      let totalCancelled = 0;

      for (const handle of handles) {
        const result = await this.cancelOne(rustClient, handle.id, handle);
        results.push(result);
        if (result.cancelled) totalCancelled++;
      }

      return transformPayload(params, {
        success: true,
        cancelled: results,
        totalCancelled,
        totalAttempted: handles.length,
      });
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      return transformPayload(params, {
        success: false,
        cancelled: [],
        totalCancelled: 0,
        totalAttempted: 0,
        error: message,
      });
    }
  }

  /**
   * Filter handles by type and status.
   * Default status filter is 'running' (only cancel active sentinels).
   */
  private filterHandles(handles: SentinelHandle[], params: SentinelCancelParams): SentinelHandle[] {
    const statusFilter = params.status ?? 'running';

    return handles.filter(h => {
      if (h.status !== statusFilter) return false;
      if (params.type && h.sentinelType !== params.type) return false;
      return true;
    });
  }

  /**
   * Cancel a single sentinel by handle ID.
   */
  private async cancelOne(
    rustClient: RustCoreIPCClient,
    handleId: string,
    handle?: SentinelHandle,
  ): Promise<CancelledSentinel> {
    try {
      const result = await rustClient.sentinelCancel(handleId);
      return {
        handle: handleId,
        type: handle?.sentinelType ?? 'unknown',
        previousStatus: handle?.status ?? 'unknown',
        cancelled: result.status === 'cancelled',
      };
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      return {
        handle: handleId,
        type: handle?.sentinelType ?? 'unknown',
        previousStatus: handle?.status ?? 'unknown',
        cancelled: false,
        error: message,
      };
    }
  }
}
