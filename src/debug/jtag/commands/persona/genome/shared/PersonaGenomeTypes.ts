/**
 * Persona Genome Command - Shared Types
 *
 * Get persona genome information including base model, layers, and traits
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

// Simple error type for result transport
export interface PersonaGenomeError {
  type: string;
  message: string;
}

// Layer info returned in results
export interface LayerInfo {
  layerId: string;
  name: string;
  traitType: string;
  weight: number;
  enabled: boolean;
  orderIndex: number;
}

/**
 * Persona Genome Command Parameters
 */
export interface PersonaGenomeParams extends CommandParams {
  // Persona ID (default: calling persona from session)
  personaId?: string;
}

/**
 * Factory function for creating PersonaGenomeParams
 */
export const createPersonaGenomeParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    personaId?: string;
  }
): PersonaGenomeParams => createPayload(context, sessionId, {
  personaId: data.personaId ?? '',
  ...data
});

/**
 * Persona Genome Command Result
 */
export interface PersonaGenomeResult extends CommandResult {
  success: boolean;
  // Persona ID
  personaId: string;
  // Persona display name
  personaName: string;
  // Base model name (e.g., 'llama-3.2-3b')
  baseModel: string;
  // Whether persona has a genome
  hasGenome: boolean;
  // Genome name if exists
  genomeName: string;
  // Genome ID if exists
  genomeId: string;
  // Number of adapter layers
  layerCount: number;
  // Array of layer info
  layers: LayerInfo[];
  // Array of unique trait types in genome
  traits: string[];
  error?: PersonaGenomeError;
}

/**
 * Factory function for creating PersonaGenomeResult with defaults
 */
export const createPersonaGenomeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    personaId?: string;
    personaName?: string;
    baseModel?: string;
    hasGenome?: boolean;
    genomeName?: string;
    genomeId?: string;
    layerCount?: number;
    layers?: LayerInfo[];
    traits?: string[];
    error?: PersonaGenomeError;
  }
): PersonaGenomeResult => createPayload(context, sessionId, {
  personaId: data.personaId ?? '',
  personaName: data.personaName ?? '',
  baseModel: data.baseModel ?? '',
  hasGenome: data.hasGenome ?? false,
  genomeName: data.genomeName ?? '',
  genomeId: data.genomeId ?? '',
  layerCount: data.layerCount ?? 0,
  layers: data.layers ?? [],
  traits: data.traits ?? [],
  ...data
});

/**
 * Smart Persona Genome-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPersonaGenomeResultFromParams = (
  params: PersonaGenomeParams,
  differences: Omit<PersonaGenomeResult, 'context' | 'sessionId'>
): PersonaGenomeResult => transformPayload(params, differences);
