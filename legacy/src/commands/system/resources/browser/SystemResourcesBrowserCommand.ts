/**
 * System Resources Command - Browser Implementation
 *
 * Query system resource usage: CPU load, memory pressure, swap, and optionally top processes by CPU and memory. Uses sysinfo for cross-platform monitoring (macOS/Linux/Windows). On Apple Silicon, memory pressure directly impacts GPU headroom since VRAM is unified.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SystemResourcesParams, SystemResourcesResult } from '../shared/SystemResourcesTypes';

export class SystemResourcesBrowserCommand extends CommandBase<SystemResourcesParams, SystemResourcesResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('system/resources', context, subpath, commander);
  }

  async execute(params: SystemResourcesParams): Promise<SystemResourcesResult> {
    console.log('🌐 BROWSER: Delegating System Resources to server');
    return await this.remoteExecute(params);
  }
}
