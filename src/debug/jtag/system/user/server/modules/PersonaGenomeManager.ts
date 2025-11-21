/**
 * PersonaGenomeManager - Handles genome CRUD operations for PersonaUser
 *
 * REFACTORING: Extracted from PersonaUser.ts (lines 899-966)
 *
 * Responsibilities:
 * - Load genome from database
 * - Update genome assignment
 * - Prepare for future genomic daemon integration (paging, activation, etc.)
 *
 * Why this exists:
 * Separates genome data access from PersonaUser core behavior, enabling:
 * - Better integration with genomic daemon
 * - Cleaner separation of concerns (data access vs orchestration)
 * - Foundation for advanced genome management (paging, LRU eviction, etc.)
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { JTAGClient } from '../../../core/client/shared/JTAGClient';
import type { UserEntity } from '../../../data/entities/UserEntity';
import type { GenomeEntity } from '../../../genome/entities/GenomeEntity';
import type { DataReadParams, DataReadResult } from '../../../../commands/data/read/shared/DataReadTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../../commands/data/update/shared/DataUpdateTypes';
import { COLLECTIONS } from '../../../data/config/DatabaseConfig';

export class PersonaGenomeManager {
  constructor(
    private readonly personaId: UUID,
    private readonly personaName: string,
    private readonly getEntity: () => UserEntity,
    private readonly getClient: () => JTAGClient | undefined
  ) {}

  /**
   * Get genome for this persona (Phase 1.2)
   * Returns null if no genome assigned or genome not found
   */
  async getGenome(): Promise<GenomeEntity | null> {
    const entity = this.getEntity();
    if (!entity.genomeId) {
      return null;
    }

    const client = this.getClient();
    if (!client) {
      console.warn(`⚠️  PersonaUser ${this.personaName}: Cannot load genome - no client`);
      return null;
    }

    try {
      const result = await client.daemons.commands.execute<DataReadParams, DataReadResult<GenomeEntity>>('data/read', {
        collection: 'genomes',
        id: entity.genomeId,
        context: client.context,
        sessionId: client.sessionId,
        backend: 'server'
      });

      if (!result.success || !result.found || !result.data) {
        console.warn(`⚠️  PersonaUser ${this.personaName}: Genome ${entity.genomeId} not found`);
        return null;
      }

      return result.data;

    } catch (error) {
      console.error(`❌ PersonaUser ${this.personaName}: Error loading genome:`, error);
      return null;
    }
  }

  /**
   * Set genome for this persona (Phase 1.2)
   * Updates the genomeId field and persists to database
   */
  async setGenome(genomeId: UUID): Promise<boolean> {
    const client = this.getClient();
    if (!client) {
      console.warn(`⚠️  PersonaUser ${this.personaName}: Cannot set genome - no client`);
      return false;
    }

    try {
      // Update entity (via getEntity() which returns reference)
      const entity = this.getEntity();
      entity.genomeId = genomeId;

      // Persist to database
      const result = await client.daemons.commands.execute<DataUpdateParams<UserEntity>, DataUpdateResult<UserEntity>>('data/update', {
        collection: COLLECTIONS.USERS,
        id: entity.id,
        data: { genomeId },
        context: client.context,
        sessionId: client.sessionId,
        backend: 'server'
      });

      if (!result.success) {
        console.error(`❌ PersonaUser ${this.personaName}: Failed to update genome: ${result.error}`);
        return false;
      }

      return true;

    } catch (error) {
      console.error(`❌ PersonaUser ${this.personaName}: Error setting genome:`, error);
      return false;
    }
  }
}
