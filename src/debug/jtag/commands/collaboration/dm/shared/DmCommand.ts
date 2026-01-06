/**
 * DM Command - Base class for getting/creating private rooms
 *
 * Set theory approach: {A, B, C} == {C, B, A}
 * Room uniqueId is deterministic based on sorted participant set.
 * This ensures we always find the same room for the same participants.
 *
 * Works with:
 * - 2 participants (classic DM)
 * - 3+ participants (group DM)
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { DmParams, DmResult } from './DmTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import crypto from 'crypto';

export abstract class DmCommand extends CommandBase<DmParams, DmResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/dm', context, subpath, commander);
  }

  /**
   * Generate deterministic DM room uniqueId from participant set
   * Uses sorted IDs and hashes for consistent, URL-safe result
   *
   * Set theory: {A, B} == {B, A} - order doesn't matter
   */
  protected generateDmUniqueId(participantIds: string[]): string {
    // Sort for determinism (set equality)
    const sorted = [...participantIds].sort();

    // Hash the sorted list for a clean, fixed-length ID
    const hash = crypto.createHash('sha256')
      .update(sorted.join(':'))
      .digest('hex')
      .slice(0, 12);

    // Prefix with participant count for readability
    const prefix = participantIds.length === 2 ? 'dm' : `dm${participantIds.length}`;
    return `${prefix}-${hash}`;
  }

  /**
   * Generate default display name from participant names
   * Can be overridden by user later
   */
  protected generateDefaultDisplayName(participantNames: string[]): string {
    if (participantNames.length <= 3) {
      return participantNames.join(' & ');
    }
    // For larger groups, truncate with count
    return `${participantNames.slice(0, 2).join(', ')} +${participantNames.length - 2}`;
  }

  /**
   * Execute the DM command (get or create room)
   */
  async execute(params: JTAGPayload): Promise<DmResult> {
    return this.executeDm(params as DmParams);
  }

  /**
   * Subclass must implement the actual DM logic
   */
  protected abstract executeDm(params: DmParams): Promise<DmResult>;
}
