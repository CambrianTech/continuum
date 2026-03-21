/**
 * Persona Allocate Command - Shared Types
 *
 * Hardware-aware persona allocation via Rust PersonaAllocator. Returns optimal persona assignments based on GPU VRAM and available API keys. Single source of truth for which personas should exist on this machine.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Persona Allocate Command Parameters
 */
export interface PersonaAllocateParams extends CommandParams {
  // List of API key env var names that are currently set (e.g., ['ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY'])
  availableApiKeys: string[];
}

/**
 * Factory function for creating PersonaAllocateParams
 */
export const createPersonaAllocateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // List of API key env var names that are currently set (e.g., ['ANTHROPIC_API_KEY', 'DEEPSEEK_API_KEY'])
    availableApiKeys: string[];
  }
): PersonaAllocateParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Persona Allocate Command Result
 */
export interface PersonaAllocateResult extends CommandResult {
  success: boolean;
  // Array of persona allocations to create
  allocations: object[];
  // Array of personas skipped (with reasons)
  skipped: object[];
  // Human-readable summary lines of the allocation decision
  summary: string[];
  // Detected GPU hardware name
  gpuName: string;
  // Total detected VRAM in GB
  totalVramGb: number;
  // GPU type: 'cuda', 'metal', or 'cpu'
  gpuType: string;
  // Recommended local model for this hardware
  localModel: string;
  error?: JTAGError;
}

/**
 * Factory function for creating PersonaAllocateResult with defaults
 */
export const createPersonaAllocateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of persona allocations to create
    allocations?: object[];
    // Array of personas skipped (with reasons)
    skipped?: object[];
    // Human-readable summary lines of the allocation decision
    summary?: string[];
    // Detected GPU hardware name
    gpuName?: string;
    // Total detected VRAM in GB
    totalVramGb?: number;
    // GPU type: 'cuda', 'metal', or 'cpu'
    gpuType?: string;
    // Recommended local model for this hardware
    localModel?: string;
    error?: JTAGError;
  }
): PersonaAllocateResult => createPayload(context, sessionId, {
  allocations: data.allocations ?? [],
  skipped: data.skipped ?? [],
  summary: data.summary ?? [],
  gpuName: data.gpuName ?? '',
  totalVramGb: data.totalVramGb ?? 0,
  gpuType: data.gpuType ?? '',
  localModel: data.localModel ?? '',
  ...data
});

/**
 * Smart Persona Allocate-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPersonaAllocateResultFromParams = (
  params: PersonaAllocateParams,
  differences: Omit<PersonaAllocateResult, 'context' | 'sessionId' | 'userId'>
): PersonaAllocateResult => transformPayload(params, differences);

/**
 * Persona Allocate — Type-safe command executor
 *
 * Usage:
 *   import { PersonaAllocate } from '...shared/PersonaAllocateTypes';
 *   const result = await PersonaAllocate.execute({ ... });
 */
export const PersonaAllocate = {
  execute(params: CommandInput<PersonaAllocateParams>): Promise<PersonaAllocateResult> {
    return Commands.execute<PersonaAllocateParams, PersonaAllocateResult>('persona/allocate', params as Partial<PersonaAllocateParams>);
  },
  commandName: 'persona/allocate' as const,
} as const;
