/**
 * Skill List Command - Server Implementation
 *
 * Lists skills with optional filters by status, scope, and creator.
 * Returns SkillEntity records from the database.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SkillListParams, SkillListResult } from '../shared/SkillListTypes';
import { createSkillListResultFromParams } from '../shared/SkillListTypes';
import { SkillEntity } from '@system/data/entities/SkillEntity';
import { ORM } from '@daemons/data-daemon/shared/ORM';
import type { UniversalFilter } from '@daemons/data-daemon/shared/DataStorageAdapter';
import { COLLECTIONS } from '@system/shared/Constants';

export class SkillListServerCommand extends CommandBase<SkillListParams, SkillListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/list', context, subpath, commander);
  }

  async execute(params: SkillListParams): Promise<SkillListResult> {
    // Build filter from optional params
    const filter: UniversalFilter = {};

    if (params.status?.trim()) {
      filter.status = params.status;
    }
    if (params.scope?.trim()) {
      filter.scope = params.scope;
    }
    if (params.createdById?.trim()) {
      filter.createdById = params.createdById;
    }

    const limit = params.limit ?? 20;

    const queryResult = await ORM.query<SkillEntity>({
      collection: COLLECTIONS.SKILLS,
      filter,
      sort: [{ field: 'createdAt', direction: 'desc' }],
      limit,
    });

    const skills = queryResult.success && queryResult.data
      ? queryResult.data.map(record => record.data)
      : [];
    const total = skills.length;

    // Build human-readable summary
    const filterDesc = Object.entries(filter)
      .map(([k, v]) => `${k}=${v}`)
      .join(', ');

    return createSkillListResultFromParams(params, {
      success: true,
      skills,
      total,
      message: total > 0
        ? `Found ${total} skill${total !== 1 ? 's' : ''}${filterDesc ? ` (${filterDesc})` : ''}`
        : `No skills found${filterDesc ? ` matching ${filterDesc}` : ''}`,
    });
  }
}
