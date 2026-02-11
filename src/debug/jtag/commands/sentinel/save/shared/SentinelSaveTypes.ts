/**
 * Sentinel Save Command - Types
 *
 * Save sentinel definitions to database for persistence and sharing.
 * Sentinels are stored in the 'sentinels' collection.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelDefinition, SentinelEntity } from '../../../../system/sentinel';

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
