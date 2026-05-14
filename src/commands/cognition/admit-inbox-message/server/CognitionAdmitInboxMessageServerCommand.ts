/**
 * cognition/admit-inbox-message — Server Implementation
 *
 * Pure pass-through to the Rust `cognition/admit-inbox-message` IPC
 * handler shipped in #1121 PR-4. Wire format: { personaId, message } →
 * { decision, engramCount, traceSeamCount }. All admission logic
 * (IsMemorable recipe, trust-boundary check, replay-protection, dedup)
 * lives in Rust (`workers/continuum-core/src/modules/cognition.rs`).
 *
 * Per CLAUDE.md "Rust-Backed Commands (IPC Mixin Pattern)" + Joel's
 * "if not UI/UX it is rust" rule: this TS file exists ONLY so the
 * recipe pipeline + ./jtag CLI can route through `Commands.execute`.
 * It is a thin bridge. No business logic. No reimplementation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type {
  CognitionAdmitInboxMessageParams,
  CognitionAdmitInboxMessageResult,
} from '../shared/CognitionAdmitInboxMessageTypes';
import { createCognitionAdmitInboxMessageResultFromParams } from '../shared/CognitionAdmitInboxMessageTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { InboxMessageRequest } from '../../../../shared/generated';

export class CognitionAdmitInboxMessageServerCommand extends CommandBase<
  CognitionAdmitInboxMessageParams,
  CognitionAdmitInboxMessageResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('cognition/admit-inbox-message', context, subpath, commander);
  }

  async execute(
    params: CognitionAdmitInboxMessageParams,
  ): Promise<CognitionAdmitInboxMessageResult> {
    if (!params.personaId || params.personaId.trim() === '') {
      throw new ValidationError(
        'personaId',
        `Missing required parameter 'personaId'. Provide the UUID of the persona whose admission gate should run. See the cognition/admit-inbox-message README for usage.`,
      );
    }
    if (!params.message || typeof params.message !== 'object') {
      throw new ValidationError(
        'message',
        `Missing required parameter 'message'. Provide an InboxMessageRequest object — the candidate inbox message to admit. See shared/generated/ipc/InboxMessageRequest.ts for shape.`,
      );
    }

    const client = await RustCoreIPCClient.getInstanceAsync();
    const { decision, engram_count, trace_seam_count } = await client.cognitionAdmitInboxMessage(
      params.personaId,
      params.message as unknown as InboxMessageRequest,
    );

    return createCognitionAdmitInboxMessageResultFromParams(params, {
      success: true,
      decision: decision as unknown as Record<string, unknown>,
      engramCount: engram_count,
      traceSeamCount: trace_seam_count,
    });
  }
}
