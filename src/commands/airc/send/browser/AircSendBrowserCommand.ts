/**
 * Airc Send Command - Browser Implementation
 *
 * Send a message to the airc mesh from inside Continuum. Wraps the airc CLI's `airc send` command — broadcasts to a channel by default, DMs a peer when peer is provided. First-class surface for the AircBridge integration (continuum#967, AGENT-BACKBONE-INTEGRATION §11.2): personas (or any caller) can publish to the cross-machine peer mesh that humans + Claude Code + Codex tabs share. Outbox direction only; inbox routing (airc → persona inbox) is a separate v0.5 follow-up requiring an embedded `airc connect` Monitor process tree.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AircSendParams, AircSendResult } from '../shared/AircSendTypes';

export class AircSendBrowserCommand extends CommandBase<AircSendParams, AircSendResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('airc/send', context, subpath, commander);
  }

  async execute(params: AircSendParams): Promise<AircSendResult> {
    console.log('🌐 BROWSER: Delegating Airc Send to server');
    return await this.remoteExecute(params);
  }
}
