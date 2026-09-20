/**
 * Claude Snapshot Command - Browser Implementation
 *
 * Saves a work-state snapshot for session continuity. Captures what Claude was doing, what's pending, and what comes next — so the next Claude instance can resume without reading 200 lines of MEMORY.md and guessing.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ClaudeSnapshotParams, ClaudeSnapshotResult } from '../shared/ClaudeSnapshotTypes';

export class ClaudeSnapshotBrowserCommand extends CommandBase<ClaudeSnapshotParams, ClaudeSnapshotResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('claude/snapshot', context, subpath, commander);
  }

  async execute(params: ClaudeSnapshotParams): Promise<ClaudeSnapshotResult> {
    console.log('🌐 BROWSER: Delegating Claude Snapshot to server');
    return await this.remoteExecute(params);
  }
}
