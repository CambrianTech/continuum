/**
 * Gpu Stats Command - Browser Implementation
 *
 * Query GPU memory manager stats including VRAM detection, per-subsystem budgets (inference, TTS, rendering), usage tracking, and memory pressure. Returns real hardware data from Metal (macOS) or CUDA APIs.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GpuStatsParams, GpuStatsResult } from '../shared/GpuStatsTypes';

export class GpuStatsBrowserCommand extends CommandBase<GpuStatsParams, GpuStatsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('gpu/stats', context, subpath, commander);
  }

  async execute(params: GpuStatsParams): Promise<GpuStatsResult> {
    console.log('🌐 BROWSER: Delegating Gpu Stats to server');
    return await this.remoteExecute(params);
  }
}
