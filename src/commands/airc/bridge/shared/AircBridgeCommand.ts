import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { AircBridgeParams, AircBridgeResult } from './AircBridgeTypes';

export abstract class AircBridgeCommand extends CommandBase<AircBridgeParams, AircBridgeResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('airc/bridge', context, subpath, commander);
  }

  protected abstract executeAircBridge(params: AircBridgeParams): Promise<AircBridgeResult>;

  async execute(params: JTAGPayload): Promise<AircBridgeResult> {
    return this.executeAircBridge(params as AircBridgeParams);
  }
}
