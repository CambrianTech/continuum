/**
 * Adapter Try Command - Shared Types
 *
 * Temporarily load a LoRA adapter and run A/B comparison test
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
// Simple error type for result transport
export interface AdapterTryError {
  type: string;
  message: string;
}
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Adapter Try Command Parameters
 */
export interface AdapterTryParams extends CommandParams {
  // Adapter ID (HuggingFace repo ID or local adapter name)
  adapterId: string;
  // Prompt to test with and without the adapter
  testPrompt: string;
  // Adapter scale/weight (default: 1.0)
  scale?: number;
  // Max tokens for test generation (default: 100)
  maxTokens?: number;
}

/**
 * Factory function for creating AdapterTryParams
 */
export const createAdapterTryParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Adapter ID (HuggingFace repo ID or local adapter name)
    adapterId: string;
    // Prompt to test with and without the adapter
    testPrompt: string;
    // Adapter scale/weight (default: 1.0)
    scale?: number;
    // Max tokens for test generation (default: 100)
    maxTokens?: number;
  }
): AdapterTryParams => createPayload(context, sessionId, {
  scale: data.scale ?? 0,
  maxTokens: data.maxTokens ?? 0,
  ...data
});

/**
 * Adapter Try Command Result
 */
export interface AdapterTryResult extends CommandResult {
  success: boolean;
  // Adapter that was tested
  adapterId: string;
  // Output without adapter
  baselineOutput: string;
  // Output with adapter loaded
  adapterOutput: string;
  // Generation time without adapter
  baselineTimeMs: number;
  // Generation time with adapter
  adapterTimeMs: number;
  // Adapter metadata (base model, rank, etc.)
  adapterMetadata: object;
  error?: AdapterTryError;
}

/**
 * Factory function for creating AdapterTryResult with defaults
 */
export const createAdapterTryResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Adapter that was tested
    adapterId?: string;
    // Output without adapter
    baselineOutput?: string;
    // Output with adapter loaded
    adapterOutput?: string;
    // Generation time without adapter
    baselineTimeMs?: number;
    // Generation time with adapter
    adapterTimeMs?: number;
    // Adapter metadata (base model, rank, etc.)
    adapterMetadata?: object;
    error?: AdapterTryError;
  }
): AdapterTryResult => createPayload(context, sessionId, {
  adapterId: data.adapterId ?? '',
  baselineOutput: data.baselineOutput ?? '',
  adapterOutput: data.adapterOutput ?? '',
  baselineTimeMs: data.baselineTimeMs ?? 0,
  adapterTimeMs: data.adapterTimeMs ?? 0,
  adapterMetadata: data.adapterMetadata ?? {},
  ...data
});

/**
 * Smart Adapter Try-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAdapterTryResultFromParams = (
  params: AdapterTryParams,
  differences: Omit<AdapterTryResult, 'context' | 'sessionId'>
): AdapterTryResult => transformPayload(params, differences);

/**
 * AdapterTry — Type-safe command executor
 *
 * Usage:
 *   import { AdapterTry } from '...shared/AdapterTryTypes';
 *   const result = await AdapterTry.execute({ ... });
 */
export const AdapterTry = {
  execute(params: CommandInput<AdapterTryParams>): Promise<AdapterTryResult> {
    return Commands.execute<AdapterTryParams, AdapterTryResult>('adapter/try', params as Partial<AdapterTryParams>);
  },
  commandName: 'adapter/try' as const,
} as const;
