/**
 * Cognition Admit Inbox Message Command - Browser Implementation
 *
 * Run the per-persona admission gate over a single InboxMessage. Returns the typed AdmissionDecision (Admit | Drop | Quarantine) plus the post-call admitted-engram count and trace seam count. Side effects: admitted engram → store, content_hash → dedup record, AIRC event_id → replay-protection record. Wraps the Rust IPC handler shipped in #1121 PR-4.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CognitionAdmitInboxMessageParams, CognitionAdmitInboxMessageResult } from '../shared/CognitionAdmitInboxMessageTypes';

export class CognitionAdmitInboxMessageBrowserCommand extends CommandBase<CognitionAdmitInboxMessageParams, CognitionAdmitInboxMessageResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('cognition/admit-inbox-message', context, subpath, commander);
  }

  async execute(params: CognitionAdmitInboxMessageParams): Promise<CognitionAdmitInboxMessageResult> {
    console.log('🌐 BROWSER: Delegating Cognition Admit Inbox Message to server');
    return await this.remoteExecute(params);
  }
}
