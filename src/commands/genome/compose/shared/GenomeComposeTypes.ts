/**
 * Genome Compose Command Types
 *
 * Compose multiple trained LoRA layers into a single stacked genome.
 * Uses GenomeAssembler for weighted merge, then optionally activates
 * the composed genome on the persona (triggering LRU eviction if needed).
 *
 * This is the "dynamic composition" step — after training N topics,
 * compose all adapters into one merged genome for inference.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { CompositionStrategy } from '../../../../system/genome/shared/GenomeAssemblyTypes';

/**
 * A single layer reference for composition
 */
export interface ComposeLayerRef {
  /** UUID of the trained GenomeLayerEntity */
  layerId: UUID;
  /** Importance weight (0.0 - 1.0, default: 1.0) */
  weight?: number;
  /** Stack ordering (lower = applied first, default: index) */
  ordering?: number;
}

export interface GenomeComposeParams extends CommandParams {
  /** Persona to compose layers for */
  personaId: UUID;
  /** Layers to compose */
  layers: ComposeLayerRef[];
  /** Base model these layers were trained on */
  baseModel: string;
  /** Name for the composed genome (default: auto-generated) */
  name?: string;
  /** Composition strategy (default: 'weighted-merge') */
  strategy?: CompositionStrategy;
  /** Auto-activate on persona after composition (default: true) */
  activate?: boolean;
}

export interface GenomeComposeResult extends CommandResult {
  success: boolean;
  /** UUID of the composed genome entity */
  genomeId?: UUID;
  /** Number of layers composed */
  layerCount: number;
  /** Composition time in milliseconds */
  compositionTimeMs: number;
  /** Whether the genome was activated on the persona */
  activated: boolean;
  /** Adapters evicted during activation (if any) */
  evictedAdapters?: UUID[];
  /** Composition strategy used */
  strategy: CompositionStrategy;
  error?: string;
}

/**
 * Helper to create GenomeComposeResult from params
 */
export const createGenomeComposeResultFromParams = (
  params: GenomeComposeParams,
  differences: Omit<Partial<GenomeComposeResult>, 'context' | 'sessionId'>
): GenomeComposeResult => transformPayload(params, {
  success: false,
  layerCount: 0,
  compositionTimeMs: 0,
  activated: false,
  strategy: params.strategy ?? 'weighted-merge',
  ...differences,
});

/**
 * GenomeCompose — Type-safe command executor
 */
export const GenomeCompose = {
  execute(params: CommandInput<GenomeComposeParams>): Promise<GenomeComposeResult> {
    return Commands.execute<GenomeComposeParams, GenomeComposeResult>('genome/compose', params as Partial<GenomeComposeParams>);
  },
  commandName: 'genome/compose' as const,
} as const;
