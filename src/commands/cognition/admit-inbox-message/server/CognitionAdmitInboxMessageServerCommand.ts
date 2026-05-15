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
 *
 * **Refactored to RustBackedCommand (#1198):** the standard validate +
 * call mixin + wrap-result envelope is now in the base class. Only the
 * variable bits — required-param list, mixin call, result mapping —
 * remain here. See `RustBackedCommand.ts` for the migration pattern.
 */

import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { RustBackedCommand } from '@daemons/command-daemon/shared/RustBackedCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type {
  CognitionAdmitInboxMessageParams,
  CognitionAdmitInboxMessageResult,
} from '../shared/CognitionAdmitInboxMessageTypes';
import { createCognitionAdmitInboxMessageResultFromParams } from '../shared/CognitionAdmitInboxMessageTypes';
import type { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { InboxMessageRequest } from '../../../../shared/generated';

/** Snake-case shape returned by the Rust mixin — matches the IPC payload. */
type AdmitInboxMessageRustResponse = {
  decision: unknown;
  engram_count: number;
  trace_seam_count: number;
};

export class CognitionAdmitInboxMessageServerCommand extends RustBackedCommand<
  CognitionAdmitInboxMessageParams,
  CognitionAdmitInboxMessageResult,
  AdmitInboxMessageRustResponse
> {
  protected override readonly requiredParams = ['personaId', 'message'] as const;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('cognition/admit-inbox-message', context, subpath, commander);
  }

  /**
   * Subclass override: `message` must be a non-null object, not just
   * truthy. The base class default checks for non-empty strings; this
   * shape constraint is command-specific.
   */
  protected override validateParams(params: CognitionAdmitInboxMessageParams): void {
    super.validateParams(params);
    if (typeof params.message !== 'object' || params.message === null) {
      throw new ValidationError(
        'message',
        `Required parameter 'message' must be an InboxMessageRequest object — ` +
          `see shared/generated/ipc/InboxMessageRequest.ts for shape.`,
      );
    }
  }

  protected override async callRust(
    params: CognitionAdmitInboxMessageParams,
    client: RustCoreIPCClient,
  ): Promise<AdmitInboxMessageRustResponse> {
    return client.cognitionAdmitInboxMessage(
      params.personaId,
      params.message as unknown as InboxMessageRequest,
    );
  }

  protected override toResult(
    raw: AdmitInboxMessageRustResponse,
    params: CognitionAdmitInboxMessageParams,
  ): CognitionAdmitInboxMessageResult {
    return createCognitionAdmitInboxMessageResultFromParams(params, {
      success: true,
      decision: raw.decision as Record<string, unknown>,
      engramCount: raw.engram_count,
      traceSeamCount: raw.trace_seam_count,
    });
  }
}
