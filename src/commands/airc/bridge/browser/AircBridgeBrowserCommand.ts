import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { AircBridgeCommand } from '../shared/AircBridgeCommand';
import type { AircBridgeParams, AircBridgeResult } from '../shared/AircBridgeTypes';

export class AircBridgeBrowserCommand extends AircBridgeCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeAircBridge(params: AircBridgeParams): Promise<AircBridgeResult> {
    return this.remoteExecute(params);
  }
}
