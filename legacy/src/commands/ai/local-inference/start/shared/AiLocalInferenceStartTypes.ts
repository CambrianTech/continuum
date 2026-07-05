/**
 * Ai Local Inference Start Command - Shared Types
 *
 * Ensure Continuum's local inference HTTP server is running and return its URL. Idempotent — if already running, returns the existing URL without restarting. External agents (Claude Code via ANTHROPIC_BASE_URL, future Codex via OPENAI_BASE_URL) should call this once at startup, then use the returned URL. First-class surface for the AGENT-BACKBONE integration story (PR #976 §1-§4); previously only reachable as the Sentinel-internal sentinel/local-inference-start IPC command.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Ai Local Inference Start Command Parameters.
 *
 * The command takes no command-specific params — `context` + `sessionId`
 * + `userId` inherited from CommandParams are the full payload shape.
 * Modeled as a type alias to CommandParams: no phantom `_noParams: never`
 * marker that lies about emptiness, no `extends CommandParams {}` that
 * adds a structurally-identical-but-distinct nominal type.
 */
export type AiLocalInferenceStartParams = CommandParams;

/**
 * Factory function for creating AiLocalInferenceStartParams.
 *
 * userId is REQUIRED on CommandParams (auto-injected by Commands.execute
 * at runtime; explicit on server-side construction). createPayload<T>
 * returns `T & JTAGPayload` which is structurally CommandParams when
 * T = `{ userId: UUID }` — no casts needed.
 */
export const createAiLocalInferenceStartParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
): AiLocalInferenceStartParams => createPayload(context, sessionId, { userId });

/**
 * Ai Local Inference Start Command Result
 */
export interface AiLocalInferenceStartResult extends CommandResult {
  success: boolean;
  // Base URL where the local inference server is accepting requests (e.g., http://127.0.0.1:8421)
  url: string;
  // TCP port the server is bound to
  port: number;
  // Wire protocol the server speaks. Currently always 'anthropic' (Messages API).
  protocol: string;
  // True if the server was already up before this call (no spawn happened); false if this call started it
  alreadyRunning: boolean;
  error?: JTAGError;
}

/**
 * Factory function for creating AiLocalInferenceStartResult with defaults
 */
export const createAiLocalInferenceStartResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Base URL where the local inference server is accepting requests (e.g., http://127.0.0.1:8421)
    url?: string;
    // TCP port the server is bound to
    port?: number;
    // Wire protocol the server speaks. Currently always 'anthropic' (Messages API).
    protocol?: string;
    // True if the server was already up before this call (no spawn happened); false if this call started it
    alreadyRunning?: boolean;
    error?: JTAGError;
  }
): AiLocalInferenceStartResult => createPayload(context, sessionId, {
  url: data.url ?? '',
  port: data.port ?? 0,
  protocol: data.protocol ?? '',
  alreadyRunning: data.alreadyRunning ?? false,
  ...data
});

/**
 * Smart Ai Local Inference Start-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiLocalInferenceStartResultFromParams = (
  params: AiLocalInferenceStartParams,
  differences: Omit<AiLocalInferenceStartResult, 'context' | 'sessionId' | 'userId'>
): AiLocalInferenceStartResult => transformPayload(params, differences);

/**
 * Ai Local Inference Start — Type-safe command executor
 *
 * Usage:
 *   import { AiLocalInferenceStart } from '...shared/AiLocalInferenceStartTypes';
 *   const result = await AiLocalInferenceStart.execute({ ... });
 */
export const AiLocalInferenceStart = {
  execute(params: CommandInput<AiLocalInferenceStartParams>): Promise<AiLocalInferenceStartResult> {
    return Commands.execute<AiLocalInferenceStartParams, AiLocalInferenceStartResult>('ai/local-inference/start', params as Partial<AiLocalInferenceStartParams>);
  },
  commandName: 'ai/local-inference/start' as const,
} as const;
