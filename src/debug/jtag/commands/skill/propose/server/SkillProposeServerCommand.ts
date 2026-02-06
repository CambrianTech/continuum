/**
 * Skill Propose Command - Server Implementation
 *
 * Creates a SkillEntity from an AI's proposed specification.
 * For team-scoped skills, also creates a DecisionProposal for governance.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { SkillProposeParams, SkillProposeResult } from '../shared/SkillProposeTypes';
import { createSkillProposeResultFromParams } from '../shared/SkillProposeTypes';
import { SkillEntity } from '@system/data/entities/SkillEntity';
import type { SkillSpec, SkillParamSpec, SkillResultSpec, SkillScope } from '@system/data/entities/SkillEntity';
import { ORM } from '@daemons/data-daemon/shared/ORM';
import { COLLECTIONS } from '@system/shared/Constants';
import { DecisionPropose } from '@commands/collaboration/decision/propose/shared/DecisionProposeTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class SkillProposeServerCommand extends CommandBase<SkillProposeParams, SkillProposeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/propose', context, subpath, commander);
  }

  async execute(params: SkillProposeParams): Promise<SkillProposeResult> {
    const { name, description, implementation, personaId } = params;
    const scope: SkillScope = (params.scope === 'team' ? 'team' : 'personal');

    if (!name?.trim()) {
      throw new ValidationError('name', "Missing required parameter 'name'. Provide the command name (e.g., 'analysis/complexity').");
    }
    if (!description?.trim()) {
      throw new ValidationError('description', "Missing required parameter 'description'.");
    }
    if (!implementation?.trim()) {
      throw new ValidationError('implementation', "Missing required parameter 'implementation'. Describe what the skill should do.");
    }
    if (!personaId?.trim()) {
      throw new ValidationError('personaId', "Missing required parameter 'personaId'.");
    }

    // Check for duplicate active skill
    const existingResult = await ORM.query<SkillEntity>({
      collection: COLLECTIONS.SKILLS,
      filter: { name, status: 'active' },
      limit: 1,
    });
    if (existingResult.success && existingResult.data && existingResult.data.length > 0) {
      throw new ValidationError('name', `A skill named '${name}' is already active.`);
    }

    // Build skill spec
    const skillParams = Array.isArray(params.skillParams) ? params.skillParams as unknown as SkillParamSpec[] : [];
    const skillResults = Array.isArray(params.skillResults) ? params.skillResults as unknown as SkillResultSpec[] : [];
    const examples = Array.isArray(params.examples)
      ? params.examples as Array<{ description: string; command: string; expectedResult?: string }>
      : undefined;

    const spec: SkillSpec = {
      name,
      description,
      params: skillParams,
      results: skillResults,
      examples,
      implementation,
      accessLevel: 'ai-safe',
    };

    // Create entity
    const entity = new SkillEntity();
    entity.name = name;
    entity.description = description;
    entity.createdById = personaId as UUID;
    entity.spec = spec;
    entity.scope = scope;
    entity.status = 'proposed';

    const validation = entity.validate();
    if (!validation.success) {
      throw new ValidationError('spec', validation.error ?? 'Skill validation failed');
    }

    // Persist
    const stored = await ORM.store<SkillEntity>(COLLECTIONS.SKILLS, entity);

    // For team-scoped skills, create a governance proposal via the decision/propose command
    let proposalId = '';
    if (scope === 'team') {
      try {
        const proposalResult = await DecisionPropose.execute({
          topic: `New Skill Proposal: ${name}`,
          rationale: `${description}\n\nImplementation: ${implementation}\n\nParams: ${JSON.stringify(spec.params)}\nResults: ${JSON.stringify(spec.results)}`,
          options: [
            { label: 'Approve', description: `Approve skill '${name}' for team use` },
            { label: 'Request Changes', description: 'Suggest modifications before approval' },
            { label: 'Reject', description: 'Decline this skill proposal' },
          ],
          scope: 'all',
          significanceLevel: 'medium',
          proposerId: personaId as UUID,
        });
        proposalId = proposalResult.proposalId ?? '';
        if (proposalId) {
          await ORM.update<SkillEntity>(
            COLLECTIONS.SKILLS,
            stored.id,
            { proposalId: proposalId as UUID } as Partial<SkillEntity>,
          );
        }
      } catch {
        // Governance proposal is optional — skill still proceeds
      }
    }

    return createSkillProposeResultFromParams(params, {
      success: true,
      skillId: stored.id,
      name: stored.name,
      status: stored.status,
      scope: stored.scope,
      proposalId,
      message: scope === 'team'
        ? `Skill '${name}' proposed for team approval (proposal: ${proposalId || 'pending'})`
        : `Skill '${name}' proposed — ready to generate`,
    });
  }
}
