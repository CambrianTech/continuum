/**
 * Genome Layers Command Types
 *
 * Query a persona's real LoRA adapter stack from AdapterStore.
 * Used by PersonaTile to render genome bars with actual data.
 */

import type { CommandParams, CommandResult, CommandInput } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

export interface GenomeLayersParams extends CommandParams {
  /** Persona ID to query adapters for */
  personaId: string;
  /** Persona display name — fallback matching when UUID changes after reseed */
  personaName?: string;
}

/** Summary of a single LoRA adapter layer */
export interface GenomeLayerInfo {
  /** Adapter name from manifest */
  name: string;
  /** Domain/trait type (e.g., 'conversational', 'code', 'voice') */
  domain: string;
  /** Whether trained weights exist on disk */
  hasWeights: boolean;
  /** Base model the adapter was trained on */
  baseModel: string;
  /** ISO timestamp of adapter creation */
  createdAt?: string;
  /** Adapter directory size in megabytes */
  sizeMB?: number;
  /** Training metrics from peft-train.py */
  trainingMetrics?: {
    finalLoss: number;
    epochs: number;
    examplesProcessed: number;
    trainingDurationMs: number;
    lossHistory?: number[];
    phenotypeScore?: number;
    phenotypeImprovement?: number;
  };
  /** Computed readiness score 0.0–1.0 */
  maturity: number;
}

export interface GenomeLayersResult extends CommandResult {
  success: boolean;
  /** Real adapter layers from AdapterStore */
  layers: GenomeLayerInfo[];
  /** Fitness score: min(1.0, trainedCount / 10) */
  fitness: number;
  error?: string;
}

export const createGenomeLayersResultFromParams = (
  params: GenomeLayersParams,
  differences: Omit<Partial<GenomeLayersResult>, 'context' | 'sessionId'>
): GenomeLayersResult => transformPayload(params, {
  success: false,
  layers: [],
  fitness: 0,
  ...differences
});

/**
 * GenomeLayers — Type-safe command executor
 *
 * Usage:
 *   import { GenomeLayers } from '...shared/GenomeLayersTypes';
 *   const result = await GenomeLayers.execute({ personaId: 'persona-helper-001' });
 */
export const GenomeLayers = {
  execute(params: CommandInput<GenomeLayersParams>): Promise<GenomeLayersResult> {
    return Commands.execute<GenomeLayersParams, GenomeLayersResult>('genome/layers', params as Partial<GenomeLayersParams>);
  },
  commandName: 'genome/layers' as const,
} as const;
