/**
 * Cognition Admit Inbox Message Command - Shared Types
 *
 * Run the per-persona admission gate over a single InboxMessage. Returns the typed AdmissionDecision (Admit | Drop | Quarantine) plus the post-call admitted-engram count and trace seam count. Side effects: admitted engram → store, content_hash → dedup record, AIRC event_id → replay-protection record. Wraps the Rust IPC handler shipped in #1121 PR-4.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';


/**
 * Cognition Admit Inbox Message Command Parameters
 */
export interface CognitionAdmitInboxMessageParams extends CommandParams {
  // UUID of the persona whose admission gate runs
  personaId: string;
  // InboxMessageRequest — the candidate inbox message to admit. Recipe pipelines pass $signal or the drained-frame entry.
  message: Record<string, unknown>;
}

/**
 * Factory function for creating CognitionAdmitInboxMessageParams
 */
export const createCognitionAdmitInboxMessageParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
  data: {
    // UUID of the persona whose admission gate runs
    personaId: string;
    // InboxMessageRequest — the candidate inbox message to admit. Recipe pipelines pass $signal or the drained-frame entry.
    message: Record<string, unknown>;
  },
): CognitionAdmitInboxMessageParams => createPayload(context, sessionId, {
  userId,
  ...data,
});

/**
 * Cognition Admit Inbox Message Command Result
 */
export interface CognitionAdmitInboxMessageResult extends CommandResult {
  success: boolean;
  // Typed AdmissionDecision (Admit | Drop | Quarantine). See shared/generated/persona/AdmissionDecision.ts for shape.
  decision: Record<string, unknown>;
  // Total engrams in the persona's admitted store after this call
  engramCount: number;
  // Number of cognition trace seams emitted during this admission
  traceSeamCount: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CognitionAdmitInboxMessageResult with defaults
 */
export const createCognitionAdmitInboxMessageResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Typed AdmissionDecision (Admit | Drop | Quarantine). See shared/generated/persona/AdmissionDecision.ts for shape.
    decision: Record<string, unknown>;
    // Total engrams in the persona's admitted store after this call
    engramCount: number;
    // Number of cognition trace seams emitted during this admission
    traceSeamCount: number;
    error?: JTAGError;
  }
): CognitionAdmitInboxMessageResult => createPayload(context, sessionId, {

  ...data
});

/**
 * Smart Cognition Admit Inbox Message-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCognitionAdmitInboxMessageResultFromParams = (
  params: CognitionAdmitInboxMessageParams,
  differences: Omit<CognitionAdmitInboxMessageResult, 'context' | 'sessionId' | 'userId'>
): CognitionAdmitInboxMessageResult => transformPayload(params, differences);

/**
 * Cognition Admit Inbox Message — Type-safe command executor
 *
 * Usage:
 *   import { CognitionAdmitInboxMessage } from '...shared/CognitionAdmitInboxMessageTypes';
 *   const result = await CognitionAdmitInboxMessage.execute({ ... });
 */
export const CognitionAdmitInboxMessage = {
  execute(params: CommandInput<CognitionAdmitInboxMessageParams>): Promise<CognitionAdmitInboxMessageResult> {
    return Commands.execute<CognitionAdmitInboxMessageParams, CognitionAdmitInboxMessageResult>('cognition/admit-inbox-message', params as Partial<CognitionAdmitInboxMessageParams>);
  },
  commandName: 'cognition/admit-inbox-message' as const,
} as const;
