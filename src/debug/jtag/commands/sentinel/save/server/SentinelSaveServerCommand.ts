/**
 * Sentinel Save Command - Server Implementation
 *
 * Saves sentinel (pipeline) definitions to the 'sentinels' collection.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { v4 as uuid } from 'uuid';
import type { SentinelSaveParams, SentinelSaveResult } from '../shared/SentinelSaveTypes';
import type { SentinelDefinition, SentinelEntity } from '../../../../system/sentinel';
import { validateDefinition, DEFAULT_ESCALATION_RULES } from '../../../../system/sentinel';

const COLLECTION = 'sentinels';

export class SentinelSaveServerCommand extends CommandBase<SentinelSaveParams, SentinelSaveResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/save', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelSaveResult> {
    const saveParams = params as SentinelSaveParams;

    // Definition is required
    if (!saveParams.definition) {
      return transformPayload(params, {
        success: false,
        error: 'Definition is required',
      });
    }

    // Parse if string (from CLI)
    let definition: SentinelDefinition;
    if (typeof saveParams.definition === 'string') {
      try {
        definition = JSON.parse(saveParams.definition);
      } catch (e) {
        return transformPayload(params, {
          success: false,
          error: 'Invalid definition JSON: ' + (e as Error).message,
        });
      }
    } else {
      definition = saveParams.definition;
    }

    // Apply overrides
    if (saveParams.name) {
      definition.name = saveParams.name;
    }
    if (saveParams.description) {
      definition.description = saveParams.description;
    }
    if (saveParams.tags) {
      definition.tags = saveParams.tags;
    }

    // Validate
    const validation = validateDefinition(definition);
    if (!validation.valid) {
      return transformPayload(params, {
        success: false,
        error: `Invalid definition: ${validation.errors.join(', ')}`,
      });
    }

    // Create entity
    const now = new Date().toISOString();
    const id = uuid();
    const entity: SentinelEntity = {
      id,
      definition,
      executions: [],
      status: 'saved',
      parentPersonaId: saveParams.parentPersonaId ?? (params as any).userId,
      createdAt: now,
      updatedAt: now,
      createdBy: (params as any).userId,
      isTemplate: saveParams.isTemplate,
      tags: saveParams.tags,
      escalationRules: saveParams.escalationRules ?? DEFAULT_ESCALATION_RULES,
      executionCount: 0,
    };

    // Save to database
    try {
      await Commands.execute('data/create', {
        collection: COLLECTION,
        data: entity,
      } as any);

      return transformPayload(params, {
        success: true,
        id,
        shortId: id.slice(0, 8),
        entity,
      });
    } catch (error: any) {
      return transformPayload(params, {
        success: false,
        error: `Failed to save: ${error.message}`,
      });
    }
  }
}
