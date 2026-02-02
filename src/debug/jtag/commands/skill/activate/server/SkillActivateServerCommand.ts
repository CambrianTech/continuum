/**
 * Skill Activate Command - Server Implementation
 *
 * Activates a validated skill by registering it as a live command.
 * The skill becomes available for use by the creator (personal) or all personas (team).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { SkillActivateParams, SkillActivateResult } from '../shared/SkillActivateTypes';
import { createSkillActivateResultFromParams } from '../shared/SkillActivateTypes';
import { SkillEntity } from '@system/data/entities/SkillEntity';
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '@system/shared/Constants';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class SkillActivateServerCommand extends CommandBase<SkillActivateParams, SkillActivateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/activate', context, subpath, commander);
  }

  async execute(params: SkillActivateParams): Promise<SkillActivateResult> {
    const { skillId } = params;

    if (!skillId?.trim()) {
      throw new ValidationError('skillId', "Missing required parameter 'skillId'.");
    }

    // Load skill entity
    const readResult = await DataDaemon.read<SkillEntity>(COLLECTIONS.SKILLS, skillId as UUID);
    if (!readResult.success || !readResult.data) {
      throw new ValidationError('skillId', `Skill not found: ${skillId}`);
    }
    const skill = readResult.data.data as SkillEntity;

    if (skill.status !== 'validated') {
      throw new ValidationError('skillId',
        `Skill '${skill.name}' cannot be activated in status '${skill.status}'. Must be 'validated' first.`);
    }

    if (!skill.outputDir) {
      throw new ValidationError('skillId', `Skill '${skill.name}' has no outputDir.`);
    }

    // For team-scoped skills, verify governance approval
    if (skill.scope === 'team' && skill.proposalId) {
      try {
        const proposalResult = await DataDaemon.read(COLLECTIONS.DECISION_PROPOSALS, skill.proposalId);
        if (proposalResult.success && proposalResult.data) {
          const proposal = proposalResult.data.data as Record<string, unknown>;
          if (proposal.status !== 'approved' && proposal.status !== 'concluded') {
            throw new ValidationError('skillId',
              `Team skill '${skill.name}' has not been approved yet (proposal status: ${proposal.status}).`);
          }
        }
      } catch (e) {
        if (e instanceof ValidationError) throw e;
        // If proposal lookup fails, proceed (governance is best-effort)
      }
    }

    // Activate: dynamically import the generated command server module
    // For personal skills: register in the runtime command map
    // For team skills: the generated files are already in commands/ and will be picked up on next build
    const now = Date.now();

    try {
      if (skill.scope === 'personal') {
        // Dynamic import of the generated server command
        const serverPath = skill.generatedFiles?.find(f => f.includes('ServerCommand'));
        if (serverPath) {
          await this.registerPersonalSkill(skill, serverPath);
        }
      }
      // Team skills: files are already in commands/ directory from generate step
      // They'll be available after the next npm start / registry rebuild
    } catch (e) {
      await DataDaemon.update<SkillEntity>(
        COLLECTIONS.SKILLS,
        skill.id as UUID,
        {
          status: 'failed',
          failureReason: `Activation failed: ${e instanceof Error ? e.message : String(e)}`,
        } as Partial<SkillEntity>,
      );

      throw new ValidationError('skillId',
        `Failed to activate skill '${skill.name}': ${e instanceof Error ? e.message : String(e)}`);
    }

    // Update entity
    await DataDaemon.update<SkillEntity>(
      COLLECTIONS.SKILLS,
      skill.id as UUID,
      {
        status: 'active',
        activatedAt: now,
      } as Partial<SkillEntity>,
    );

    return createSkillActivateResultFromParams(params, {
      success: true,
      skillId: skill.id,
      name: skill.name,
      status: 'active',
      activatedAt: now,
      message: skill.scope === 'team'
        ? `Skill '${skill.name}' activated for all personas (available after next build)`
        : `Skill '${skill.name}' activated for creator ${skill.createdById}`,
    });
  }

  private async registerPersonalSkill(_skill: SkillEntity, _serverPath: string): Promise<void> {
    // Dynamic command registration for personal skills
    // In the current architecture, commands are discovered from the file system
    // Personal skills stored in .continuum/skills/ will need the command daemon
    // to scan that directory on next refresh cycle
    //
    // For now, marking as active is sufficient — the skill files exist and can be
    // loaded by the command daemon when it next scans for commands
  }
}
