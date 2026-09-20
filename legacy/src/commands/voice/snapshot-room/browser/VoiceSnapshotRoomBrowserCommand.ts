import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VoiceSnapshotRoomParams, VoiceSnapshotRoomResult } from '../shared/VoiceSnapshotRoomTypes';

export class VoiceSnapshotRoomBrowserCommand extends CommandBase<VoiceSnapshotRoomParams, VoiceSnapshotRoomResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/snapshot-room', context, subpath, commander);
  }

  async execute(_params: VoiceSnapshotRoomParams): Promise<VoiceSnapshotRoomResult> {
    return await this.remoteExecute(_params);
  }
}

export default VoiceSnapshotRoomBrowserCommand;
