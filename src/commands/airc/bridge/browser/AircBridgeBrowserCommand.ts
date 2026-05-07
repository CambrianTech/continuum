/**
 * Airc Bridge Command - Browser Implementation
 *
 * Ingest one AIRC message into Continuum. Normal messages become chat; explicit !continuum directives become bounded development and test commands. This is the inbox-side companion to airc/send: it lets AIRC peers drive Continuum validation without shelling through jtag chat/send or chat/export by hand.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AircBridgeParams, AircBridgeResult } from '../shared/AircBridgeTypes';

export class AircBridgeBrowserCommand extends CommandBase<AircBridgeParams, AircBridgeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('airc/bridge', context, subpath, commander);
  }

  async execute(params: AircBridgeParams): Promise<AircBridgeResult> {
    console.log('🌐 BROWSER: Delegating Airc Bridge to server');
    return await this.remoteExecute(params);
  }
}
