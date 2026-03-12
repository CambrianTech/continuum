/**
 * Save sentinel definitions to database for persistence and sharing.
 * Sentinels are stored in the 'sentinels' collection.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelDefinition, SentinelEntity } from '../../../../system/sentinel';
import type { EscalationRule } from '../../../../system/sentinel';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

/**
 * Save params - accepts either:
 * 1. A complete definition object
 * 2. A handle from a previous run (captures the config used)
 */
export interface SentinelSaveParams extends CommandParams {
  /** Complete sentinel definition to save */
  definition?: SentinelDefinition;

  /** Handle from sentinel/run to capture its definition */
  handle?: string;

  /** Override the name (optional) */
  name?: string;

  /** Description (optional) */
  description?: string;

  /** Tags for organization (optional) */
  tags?: string[];

  /** Mark as template for cloning (optional) */
  isTemplate?: boolean;

  /** Owning persona — every sentinel belongs to a persona */
  parentPersonaId?: string;

  /** Escalation rules — when to alert the owning persona */
  escalationRules?: EscalationRule[];
}

/**
 * Save result
 */
export interface SentinelSaveResult extends CommandResult {
  /** Whether save succeeded */
  success: boolean;

  /** Saved entity ID */
  id?: string;

  /** Short handle for easy reference */
  shortId?: string;

  /** The saved entity */
  entity?: SentinelEntity;

  /** Error message if failed */
  error?: string;
}

/**
 * SentinelSave — Type-safe command executor
 *
 * Usage:
 *   import { SentinelSave } from '...shared/SentinelSaveTypes';
 *   const result = await SentinelSave.execute({ ... });
 */
export const SentinelSave = {
  execute(params: CommandInput<SentinelSaveParams>): Promise<SentinelSaveResult> {
    return Commands.execute<SentinelSaveParams, SentinelSaveResult>('sentinel/save', params as Partial<SentinelSaveParams>);
  },
  commandName: 'sentinel/save' as const,
} as const;
