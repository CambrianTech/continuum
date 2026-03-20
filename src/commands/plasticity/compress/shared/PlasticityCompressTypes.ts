/**
 * Plasticity Compress Command - Shared Types
 *
 * Compress a model using utilization-aware head pruning + mixed quantization. Takes a base model + gradient capture data + target device spec, produces an optimized GGUF file that fits the device's memory budget. Precision is allocated per-tensor based on head utilization scores.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Plasticity Compress Command Parameters
 */
export interface PlasticityCompressParams extends CommandParams {
  // Path to gradient capture directory (containing head_topology.json from plasticity/analyze)
  capturePath: string;
  // Path to base model safetensors directory (or HuggingFace model ID)
  modelPath: string;
  // Target device: '16gb', '32gb', '24gb-vram', '5090', 'macbookair', 'macbookpro', or JSON DeviceSpec. Default: '32gb'
  deviceSpec?: string;
  // Output GGUF file path. Default: ~/.continuum/genome/models/<model>-compressed.gguf
  outputPath?: string;
  // Model architecture: 'qwen2', 'llama'. Auto-detected from config.json if not specified.
  architecture?: string;
}

/**
 * Factory function for creating PlasticityCompressParams
 */
export const createPlasticityCompressParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to gradient capture directory (containing head_topology.json from plasticity/analyze)
    capturePath: string;
    // Path to base model safetensors directory (or HuggingFace model ID)
    modelPath: string;
    // Target device: '16gb', '32gb', '24gb-vram', '5090', 'macbookair', 'macbookpro', or JSON DeviceSpec. Default: '32gb'
    deviceSpec?: string;
    // Output GGUF file path. Default: ~/.continuum/genome/models/<model>-compressed.gguf
    outputPath?: string;
    // Model architecture: 'qwen2', 'llama'. Auto-detected from config.json if not specified.
    architecture?: string;
  }
): PlasticityCompressParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  deviceSpec: data.deviceSpec ?? '',
  outputPath: data.outputPath ?? '',
  architecture: data.architecture ?? '',
  ...data
});

/**
 * Plasticity Compress Command Result
 */
export interface PlasticityCompressResult extends CommandResult {
  success: boolean;
  // Path to the output compressed GGUF file
  ggufPath: string;
  // Output file size in GB
  outputSizeGb: number;
  // Compression ratio vs original BF16 (e.g., 4.8x)
  compressionRatio: number;
  // Count of tensors at each quantization level
  quantDistribution: object;
  // Whether the output passed verification (NaN check, dimension check)
  verified: boolean;
  error?: JTAGError;
}

/**
 * Factory function for creating PlasticityCompressResult with defaults
 */
export const createPlasticityCompressResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Path to the output compressed GGUF file
    ggufPath?: string;
    // Output file size in GB
    outputSizeGb?: number;
    // Compression ratio vs original BF16 (e.g., 4.8x)
    compressionRatio?: number;
    // Count of tensors at each quantization level
    quantDistribution?: object;
    // Whether the output passed verification (NaN check, dimension check)
    verified?: boolean;
    error?: JTAGError;
  }
): PlasticityCompressResult => createPayload(context, sessionId, {
  ggufPath: data.ggufPath ?? '',
  outputSizeGb: data.outputSizeGb ?? 0,
  compressionRatio: data.compressionRatio ?? 0,
  quantDistribution: data.quantDistribution ?? {},
  verified: data.verified ?? false,
  ...data
});

/**
 * Smart Plasticity Compress-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPlasticityCompressResultFromParams = (
  params: PlasticityCompressParams,
  differences: Omit<PlasticityCompressResult, 'context' | 'sessionId' | 'userId'>
): PlasticityCompressResult => transformPayload(params, differences);

/**
 * Plasticity Compress — Type-safe command executor
 *
 * Usage:
 *   import { PlasticityCompress } from '...shared/PlasticityCompressTypes';
 *   const result = await PlasticityCompress.execute({ ... });
 */
export const PlasticityCompress = {
  execute(params: CommandInput<PlasticityCompressParams>): Promise<PlasticityCompressResult> {
    return Commands.execute<PlasticityCompressParams, PlasticityCompressResult>('plasticity/compress', params as Partial<PlasticityCompressParams>);
  },
  commandName: 'plasticity/compress' as const,
} as const;
