/**
 * Avatar Snapshot Command - Server Implementation
 *
 * Delegates to Rust continuum-core's AvatarModule via IPC.
 * The Rust side handles Bevy slot allocation, frame capture, and PNG encoding.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AvatarSnapshotParams, AvatarSnapshotResult } from '../shared/AvatarSnapshotTypes';
import { createAvatarSnapshotResultFromParams } from '../shared/AvatarSnapshotTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class AvatarSnapshotServerCommand extends CommandBase<AvatarSnapshotParams, AvatarSnapshotResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('avatar/snapshot', context, subpath, commander);
  }

  async execute(params: AvatarSnapshotParams): Promise<AvatarSnapshotResult> {
    if (!params.identity || params.identity.trim() === '') {
      throw new ValidationError(
        'identity',
        `Missing required parameter 'identity'. Provide the persona identity (e.g., 'helper', 'teacher').`
      );
    }

    const client = RustCoreIPCClient.getInstance();
    const result = await client.avatarSnapshot(
      params.identity,
      params.width,
      params.height,
      params.force,
    );

    return createAvatarSnapshotResultFromParams(params, {
      success: true,
      path: result.path,
      cached: result.cached,
    });
  }
}
