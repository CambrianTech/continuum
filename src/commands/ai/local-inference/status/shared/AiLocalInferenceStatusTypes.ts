/**
 * Ai Local Inference Status Command - Shared Types
 *
 * Query Continuum's local inference HTTP server (Anthropic-compatible Messages API). Returns whether the server is running and the URL external agents (Claude Code via ANTHROPIC_BASE_URL, future Codex via OPENAI_BASE_URL) should point at to use local Continuum models instead of cloud APIs. First-class surface for the AGENT-BACKBONE integration story (PR #976 §1-§4).
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Ai Local Inference Status Command Parameters.
 *
 * The command takes no command-specific params — `context` + `sessionId`
 * + `userId` inherited from CommandParams are the full payload shape.
 * Modeled as a type alias to CommandParams: no phantom `_noParams: never`
 * marker that lies about emptiness, no `extends CommandParams {}` that
 * adds a structurally-identical-but-distinct nominal type.
 */
export type AiLocalInferenceStatusParams = CommandParams;

/**
 * Factory function for creating AiLocalInferenceStatusParams.
 *
 * userId is REQUIRED on CommandParams (auto-injected by Commands.execute
 * at runtime; explicit on server-side construction). createPayload<T>
 * returns `T & JTAGPayload` which is structurally CommandParams when
 * T = `{ userId: UUID }` — no casts needed.
 */
export const createAiLocalInferenceStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
): AiLocalInferenceStatusParams => createPayload(context, sessionId, { userId });

/**
 * Ai Local Inference Status Command Result
 */
export interface AiLocalInferenceStatusResult extends CommandResult {
  success: boolean;
  // True if the local inference HTTP server is bound + accepting requests
  running: boolean;
  // Base URL to use for external-agent ANTHROPIC_BASE_URL injection (e.g., http://127.0.0.1:8421). Empty when running=false.
  url: string;
  // TCP port the server is bound to. 0 when running=false.
  port: number;
  // Wire protocol the server speaks. Currently always 'anthropic' (Messages API). 'openai' will be added when openai_compat.rs lands per AGENT-BACKBONE §4.1.
  protocol: string;
  error?: JTAGError;
}

/**
 * Factory function for creating AiLocalInferenceStatusResult with defaults
 */
export const createAiLocalInferenceStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // True if the local inference HTTP server is bound + accepting requests
    running?: boolean;
    // Base URL to use for external-agent ANTHROPIC_BASE_URL injection (e.g., http://127.0.0.1:8421). Empty when running=false.
    url?: string;
    // TCP port the server is bound to. 0 when running=false.
    port?: number;
    // Wire protocol the server speaks. Currently always 'anthropic' (Messages API). 'openai' will be added when openai_compat.rs lands per AGENT-BACKBONE §4.1.
    protocol?: string;
    error?: JTAGError;
  }
): AiLocalInferenceStatusResult => createPayload(context, sessionId, {
  running: data.running ?? false,
  url: data.url ?? '',
  port: data.port ?? 0,
  protocol: data.protocol ?? '',
  ...data
});

/**
 * Smart Ai Local Inference Status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiLocalInferenceStatusResultFromParams = (
  params: AiLocalInferenceStatusParams,
  differences: Omit<AiLocalInferenceStatusResult, 'context' | 'sessionId' | 'userId'>
): AiLocalInferenceStatusResult => transformPayload(params, differences);

/**
 * Ai Local Inference Status — Type-safe command executor
 *
 * Usage:
 *   import { AiLocalInferenceStatus } from '...shared/AiLocalInferenceStatusTypes';
 *   const result = await AiLocalInferenceStatus.execute({ ... });
 */
export const AiLocalInferenceStatus = {
  execute(params: CommandInput<AiLocalInferenceStatusParams>): Promise<AiLocalInferenceStatusResult> {
    return Commands.execute<AiLocalInferenceStatusParams, AiLocalInferenceStatusResult>('ai/local-inference/status', params as Partial<AiLocalInferenceStatusParams>);
  },
  commandName: 'ai/local-inference/status' as const,
} as const;
