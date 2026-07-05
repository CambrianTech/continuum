/**
 * Genome Convert Command - Shared Types
 *
 * Convert LoRA adapters between formats. Supports: merge LoRA into full-precision model, merge + quantize to GGUF, quantize base model to GGUF, and validate converted models. Uses convert-adapter.py via Rust sentinel for process isolation.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Convert Command Parameters
 */
export interface GenomeConvertParams extends CommandParams {
  // Conversion operation: 'merge-full' (LoRA → merged FP16), 'merge-and-quantize' (LoRA → merged GGUF), 'quantize-base' (HF → GGUF), 'validate' (sanity check)
  operation: string;
  // Path to LoRA adapter directory (required for merge-full and merge-and-quantize)
  adapterPath?: string;
  // Base model name or HuggingFace ID (required for merge and quantize operations)
  baseModel?: string;
  // Quantization bits: 4 or 8 (default: 4, only for quantize operations)
  bits?: number;
  // Output directory. Default: sibling directory with format suffix
  outputPath?: string;
  // Run validation inference after conversion (default: true)
  validate?: boolean;
}

/**
 * Factory function for creating GenomeConvertParams
 */
export const createGenomeConvertParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Conversion operation: 'merge-full' (LoRA → merged FP16), 'merge-and-quantize' (LoRA → merged GGUF), 'quantize-base' (HF → GGUF), 'validate' (sanity check)
    operation: string;
    // Path to LoRA adapter directory (required for merge-full and merge-and-quantize)
    adapterPath?: string;
    // Base model name or HuggingFace ID (required for merge and quantize operations)
    baseModel?: string;
    // Quantization bits: 4 or 8 (default: 4, only for quantize operations)
    bits?: number;
    // Output directory. Default: sibling directory with format suffix
    outputPath?: string;
    // Run validation inference after conversion (default: true)
    validate?: boolean;
  }
): GenomeConvertParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  adapterPath: data.adapterPath ?? '',
  baseModel: data.baseModel ?? '',
  bits: data.bits ?? 0,
  outputPath: data.outputPath ?? '',
  validate: data.validate ?? false,
  ...data
});

/**
 * Genome Convert Command Result
 */
export interface GenomeConvertResult extends CommandResult {
  success: boolean;
  // Path to converted model/adapter
  outputPath: string;
  // Output format: 'safetensors-fp16', 'gguf-q4_0', 'gguf-q8_0'
  format: string;
  // Output size in megabytes
  sizeMB: number;
  // Conversion duration in seconds
  durationSeconds: number;
  // Original size / converted size (for quantize operations)
  compressionRatio: number;
  // Validation result if --validate was run
  validation?: Record<string, unknown>;
  error?: JTAGError;
}

/**
 * Factory function for creating GenomeConvertResult with defaults
 */
export const createGenomeConvertResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Path to converted model/adapter
    outputPath?: string;
    // Output format: 'safetensors-fp16', 'gguf-q4_0', 'gguf-q8_0'
    format?: string;
    // Output size in megabytes
    sizeMB?: number;
    // Conversion duration in seconds
    durationSeconds?: number;
    // Original size / converted size (for quantize operations)
    compressionRatio?: number;
    // Validation result if --validate was run
    validation?: Record<string, unknown>;
    error?: JTAGError;
  }
): GenomeConvertResult => createPayload(context, sessionId, {
  outputPath: data.outputPath ?? '',
  format: data.format ?? '',
  sizeMB: data.sizeMB ?? 0,
  durationSeconds: data.durationSeconds ?? 0,
  compressionRatio: data.compressionRatio ?? 0,
  validation: data.validation ?? undefined,
  ...data
});

/**
 * Smart Genome Convert-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createGenomeConvertResultFromParams = (
  params: GenomeConvertParams,
  differences: Omit<GenomeConvertResult, 'context' | 'sessionId' | 'userId'>
): GenomeConvertResult => transformPayload(params, differences);

/**
 * Genome Convert — Type-safe command executor
 *
 * Usage:
 *   import { GenomeConvert } from '...shared/GenomeConvertTypes';
 *   const result = await GenomeConvert.execute({ ... });
 */
export const GenomeConvert = {
  execute(params: CommandInput<GenomeConvertParams>): Promise<GenomeConvertResult> {
    return Commands.execute<GenomeConvertParams, GenomeConvertResult>('genome/convert', params as Partial<GenomeConvertParams>);
  },
  commandName: 'genome/convert' as const,
} as const;
