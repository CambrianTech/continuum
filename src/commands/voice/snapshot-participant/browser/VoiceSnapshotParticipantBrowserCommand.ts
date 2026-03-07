import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VoiceSnapshotParticipantParams, VoiceSnapshotParticipantResult } from '../shared/VoiceSnapshotParticipantTypes';

export class VoiceSnapshotParticipantBrowserCommand extends CommandBase<VoiceSnapshotParticipantParams, VoiceSnapshotParticipantResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/snapshot-participant', context, subpath, commander);
  }

  async execute(params: VoiceSnapshotParticipantParams): Promise<VoiceSnapshotParticipantResult> {
    return await this.remoteExecute(params);
  }
}

export default VoiceSnapshotParticipantBrowserCommand;
