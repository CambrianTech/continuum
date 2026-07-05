/**
 * Avatar Snapshot Command - Browser Implementation
 *
 * Capture a Bevy 3D avatar snapshot as PNG for profile pictures. Allocates a temporary render slot, loads the persona's VRM model, waits for a clean frame, encodes as PNG, and saves to ~/.continuum/avatars/. Cached on disk — subsequent calls return immediately unless force=true.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AvatarSnapshotParams, AvatarSnapshotResult } from '../shared/AvatarSnapshotTypes';

export class AvatarSnapshotBrowserCommand extends CommandBase<AvatarSnapshotParams, AvatarSnapshotResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('avatar/snapshot', context, subpath, commander);
  }

  async execute(params: AvatarSnapshotParams): Promise<AvatarSnapshotResult> {
    console.log('🌐 BROWSER: Delegating Avatar Snapshot to server');
    return await this.remoteExecute(params);
  }
}
