import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { VoiceSnapshotRoomParams, VoiceSnapshotRoomResult } from '../shared/VoiceSnapshotRoomTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class VoiceSnapshotRoomServerCommand extends CommandBase<VoiceSnapshotRoomParams, VoiceSnapshotRoomResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('voice/snapshot-room', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
    this.rustClient.connect().catch(() => {});
  }

  async execute(_params: VoiceSnapshotRoomParams): Promise<VoiceSnapshotRoomResult> {
    const result = await this.rustClient.voiceSnapshotRoom();
    return result as VoiceSnapshotRoomResult;
  }
}

export default VoiceSnapshotRoomServerCommand;
