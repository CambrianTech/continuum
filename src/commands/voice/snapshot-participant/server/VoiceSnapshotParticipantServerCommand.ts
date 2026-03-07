import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { VoiceSnapshotParticipantParams, VoiceSnapshotParticipantResult } from '../shared/VoiceSnapshotParticipantTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class VoiceSnapshotParticipantServerCommand extends CommandBase<VoiceSnapshotParticipantParams, VoiceSnapshotParticipantResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/snapshot-participant', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
    this.rustClient.connect().catch(() => {});
  }

  async execute(params: VoiceSnapshotParticipantParams): Promise<VoiceSnapshotParticipantResult> {
    if (!params.identity) {
      throw new ValidationError('identity', 'identity parameter is required');
    }
    const result = await this.rustClient.voiceSnapshotParticipant(params.identity);
    return result as VoiceSnapshotParticipantResult;
  }
}

export default VoiceSnapshotParticipantServerCommand;
