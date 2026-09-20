/**
 * Adapter Adopt Command - Shared Types
 *
 * Add an adapter to a persona's genome, making it a permanent trait
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

// Simple error type for result transport
export interface AdapterAdoptError {
  type: string;
  message: string;
}

/**
 * Adapter Adopt Command Parameters
 */
export interface AdapterAdoptParams extends CommandParams {
  // Adapter ID (HuggingFace repo ID or local adapter name)
  adapterId: string;
  // Adapter weight/scale (0-1, default: 1.0)
  scale?: number;
  // Domain/trait type (e.g., 'code', 'tone', 'domain_expertise')
  traitType?: string;
  // Target persona ID (default: calling persona)
  personaId?: string;
}

/**
 * Factory function for creating AdapterAdoptParams
 */
export const createAdapterAdoptParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Adapter ID (HuggingFace repo ID or local adapter name)
    adapterId: string;
    // Adapter weight/scale (0-1, default: 1.0)
    scale?: number;
    // Domain/trait type (e.g., 'code', 'tone', 'domain_expertise')
    traitType?: string;
    // Target persona ID (default: calling persona)
    personaId?: string;
  }
): AdapterAdoptParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  scale: data.scale ?? 0,
  traitType: data.traitType ?? '',
  personaId: data.personaId ?? '',
  ...data
});

/**
 * Adapter Adopt Command Result
 */
export interface AdapterAdoptResult extends CommandResult {
  success: boolean;
  // Adopted adapter ID
  adapterId: string;
  // Genome layer ID created
  layerId: string;
  // Persona that adopted the adapter
  personaId: string;
  // Name of persona's genome
  genomeName: string;
  // Number of layers in genome
  layerCount: number;
  // Adapter metadata (rank, base model, etc.)
  metadata: object;
  error?: AdapterAdoptError;
}

/**
 * Factory function for creating AdapterAdoptResult with defaults
 */
export const createAdapterAdoptResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Adopted adapter ID
    adapterId?: string;
    // Genome layer ID created
    layerId?: string;
    // Persona that adopted the adapter
    personaId?: string;
    // Name of persona's genome
    genomeName?: string;
    // Number of layers in genome
    layerCount?: number;
    // Adapter metadata (rank, base model, etc.)
    metadata?: object;
    error?: AdapterAdoptError;
  }
): AdapterAdoptResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  adapterId: data.adapterId ?? '',
  layerId: data.layerId ?? '',
  personaId: data.personaId ?? '',
  genomeName: data.genomeName ?? '',
  layerCount: data.layerCount ?? 0,
  metadata: data.metadata ?? {},
  ...data  // success comes from ...data
});

/**
 * Smart Adapter Adopt-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAdapterAdoptResultFromParams = (
  params: AdapterAdoptParams,
  differences: Omit<AdapterAdoptResult, 'context' | 'sessionId'>
): AdapterAdoptResult => transformPayload(params, differences);

/**
 * AdapterAdopt — Type-safe command executor
 *
 * Usage:
 *   import { AdapterAdopt } from '...shared/AdapterAdoptTypes';
 *   const result = await AdapterAdopt.execute({ ... });
 */
export const AdapterAdopt = {
  execute(params: CommandInput<AdapterAdoptParams>): Promise<AdapterAdoptResult> {
    return Commands.execute<AdapterAdoptParams, AdapterAdoptResult>('adapter/adopt', params as Partial<AdapterAdoptParams>);
  },
  commandName: 'adapter/adopt' as const,
} as const;
