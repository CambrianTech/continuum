/**
 * Genome Adapter Info Command - Shared Types
 *
 * Get detailed information about a specific LoRA adapter.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';

export interface AdapterTrainingInfo {
  epochs: number;
  loss: number;
  performance: number;
  trainingDurationMs: number;
  datasetHash?: string;
}

export interface AdapterArchitectureInfo {
  peftType: string;
  rank: number;
  loraAlpha: number;
  loraDropout: number;
  targetModules: string[];
  bias: string;
}

export interface AdapterCompatibilityInfo {
  baseModel: string;
  quantizationEnabled: boolean;
  quantizationBits?: number;
  quantizationType?: string;
}

export interface GenomeAdapterInfoParams extends CommandParams {
  name: string;
}

export interface GenomeAdapterInfoResult extends CommandResult {
  success: boolean;
  name: string;
  domain: string;
  sizeMB: number;
  baseModel: string;
  createdAt: string;
  lastUsedAt?: string;
  dirPath: string;
  personaId: string;
  personaName: string;
  hasWeights: boolean;
  isActive: boolean;
  trainingInfo?: AdapterTrainingInfo;
  architecture?: AdapterArchitectureInfo;
  compatibility: AdapterCompatibilityInfo;
  error?: JTAGError;
}

export const createGenomeAdapterInfoResultFromParams = (
  params: GenomeAdapterInfoParams,
  differences: Omit<GenomeAdapterInfoResult, 'context' | 'sessionId' | 'userId'>
): GenomeAdapterInfoResult => transformPayload(params, differences);

export const GenomeAdapterInfo = {
  execute(params: CommandInput<GenomeAdapterInfoParams>): Promise<GenomeAdapterInfoResult> {
    return Commands.execute<GenomeAdapterInfoParams, GenomeAdapterInfoResult>('genome/adapter-info', params as Partial<GenomeAdapterInfoParams>);
  },
  commandName: 'genome/adapter-info' as const,
} as const;
