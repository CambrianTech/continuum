/**
 * Persona Genome Command - Server Implementation
 *
 * Get persona genome information including base model, layers, and traits.
 * Essential for personas to know their own identity and capabilities.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PersonaGenomeParams, PersonaGenomeResult, LayerInfo } from '../shared/PersonaGenomeTypes';
import { createPersonaGenomeResultFromParams } from '../shared/PersonaGenomeTypes';
import { Commands } from '@system/core/shared/Commands';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import type { DataReadParams, DataReadResult } from '@commands/data/read/shared/DataReadTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { UserEntity } from '@system/data/entities/UserEntity';
import type { GenomeEntity } from '@system/genome/entities/GenomeEntity';
import type { GenomeLayerEntity } from '@system/genome/entities/GenomeLayerEntity';

import { DataRead } from '../../../data/read/shared/DataReadTypes';
export class PersonaGenomeServerCommand extends CommandBase<PersonaGenomeParams, PersonaGenomeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/genome', context, subpath, commander);
  }

  async execute(params: PersonaGenomeParams): Promise<PersonaGenomeResult> {
    console.log('🧬 Persona Genome: Querying genome info');

    try {
      // Get persona ID from params or try to get from session
      let personaId = params.personaId;

      if (!personaId) {
        // Try to get from session (for persona calling about themselves)
        // TODO: Implement session-based persona ID resolution
        // For now, require explicit personaId
        return createPersonaGenomeResultFromParams(params, {
          success: false,
          personaId: '',
          personaName: '',
          baseModel: 'llama-3.2-3b', // Default base model
          hasGenome: false,
          genomeName: '',
          genomeId: '',
          layerCount: 0,
          layers: [],
          traits: [],
          error: {
            type: 'missing_persona_id',
            message: 'No personaId provided. Specify personaId parameter.',
          },
        });
      }

      // Look up the persona
      const personaResult = await DataRead.execute<UserEntity>({
          collection: COLLECTIONS.USERS,
          id: personaId,
        }
      ) as DataReadResult<UserEntity>;

      if (!personaResult.success || !personaResult.found || !personaResult.data) {
        return createPersonaGenomeResultFromParams(params, {
          success: false,
          personaId,
          personaName: '',
          baseModel: 'llama-3.2-3b',
          hasGenome: false,
          genomeName: '',
          genomeId: '',
          layerCount: 0,
          layers: [],
          traits: [],
          error: {
            type: 'persona_not_found',
            message: `Persona not found: ${personaId}`,
          },
        });
      }

      const persona = personaResult.data;
      // Note: ID is at result.id, not result.data.id (DataReadResult structure)
      const resolvedPersonaId = personaResult.id;

      // If no genome, return basic info with default base model
      if (!persona.genomeId) {
        return createPersonaGenomeResultFromParams(params, {
          success: true,
          personaId: resolvedPersonaId,
          personaName: persona.displayName,
          baseModel: 'llama-3.2-3b', // Default
          hasGenome: false,
          genomeName: '',
          genomeId: '',
          layerCount: 0,
          layers: [],
          traits: [],
        });
      }

      // Load the genome
      const genomeResult = await DataRead.execute<GenomeEntity>({
          collection: 'genomes',
          id: persona.genomeId,
        }
      ) as DataReadResult<GenomeEntity>;

      if (!genomeResult.success || !genomeResult.found || !genomeResult.data) {
        return createPersonaGenomeResultFromParams(params, {
          success: true,
          personaId: resolvedPersonaId,
          personaName: persona.displayName,
          baseModel: 'llama-3.2-3b',
          hasGenome: false,
          genomeName: '',
          genomeId: persona.genomeId,
          layerCount: 0,
          layers: [],
          traits: [],
          error: {
            type: 'genome_not_found',
            message: `Genome referenced but not found: ${persona.genomeId}`,
          },
        });
      }

      const genome = genomeResult.data;

      // Load layer details to get names
      const layerInfos: LayerInfo[] = [];
      const traits = new Set<string>();

      for (const layerRef of genome.layers) {
        const layerResult = await DataRead.execute<GenomeLayerEntity>({
            collection: 'genome_layers',
            id: layerRef.layerId,
          }
        ) as DataReadResult<GenomeLayerEntity>;

        if (layerResult.success && layerResult.found && layerResult.data) {
          const layer = layerResult.data;
          layerInfos.push({
            layerId: layer.id,
            name: layer.name,
            traitType: layerRef.traitType,
            weight: layerRef.weight,
            enabled: layerRef.enabled,
            orderIndex: layerRef.orderIndex,
          });
          traits.add(layerRef.traitType);
        } else {
          // Layer reference exists but layer not found - include partial info
          layerInfos.push({
            layerId: layerRef.layerId,
            name: '(unknown)',
            traitType: layerRef.traitType,
            weight: layerRef.weight,
            enabled: layerRef.enabled,
            orderIndex: layerRef.orderIndex,
          });
          traits.add(layerRef.traitType);
        }
      }

      // Sort by orderIndex
      layerInfos.sort((a, b) => a.orderIndex - b.orderIndex);

      console.log(`   ✅ Genome info: ${genome.name}, ${layerInfos.length} layers, base: ${genome.baseModel}`);

      return createPersonaGenomeResultFromParams(params, {
        success: true,
        personaId: resolvedPersonaId,
        personaName: persona.displayName,
        baseModel: genome.baseModel,
        hasGenome: true,
        genomeName: genome.name,
        genomeId: genomeResult.id,
        layerCount: layerInfos.length,
        layers: layerInfos,
        traits: Array.from(traits),
      });

    } catch (error) {
      console.error('   ❌ Persona genome query failed:', error);
      return createPersonaGenomeResultFromParams(params, {
        success: false,
        personaId: params.personaId || '',
        personaName: '',
        baseModel: 'llama-3.2-3b',
        hasGenome: false,
        genomeName: '',
        genomeId: '',
        layerCount: 0,
        layers: [],
        traits: [],
        error: {
          type: 'query_failed',
          message: String(error),
        },
      });
    }
  }
}
