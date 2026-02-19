/**
 * Skill Generate Command - Server Implementation
 *
 * Retrieves a SkillEntity and runs CommandGenerator programmatically
 * to produce the command source files.
 */

import * as path from 'path';
import * as fs from 'fs';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { SkillGenerateParams, SkillGenerateResult } from '../shared/SkillGenerateTypes';
import { createSkillGenerateResultFromParams } from '../shared/SkillGenerateTypes';
import { SkillEntity } from '@system/data/entities/SkillEntity';
import { ORM } from '@daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '@system/shared/Constants';
import { CommandGenerator } from '@generator/CommandGenerator';
import type { CommandSpec } from '@generator/CommandNaming';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class SkillGenerateServerCommand extends CommandBase<SkillGenerateParams, SkillGenerateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('skill/generate', context, subpath, commander);
  }

  async execute(params: SkillGenerateParams): Promise<SkillGenerateResult> {
    const { skillId } = params;

    if (!skillId?.trim()) {
      throw new ValidationError('skillId', "Missing required parameter 'skillId'.");
    }

    // Load skill entity
    const skill = await ORM.read<SkillEntity>(COLLECTIONS.SKILLS, skillId as UUID);
    if (!skill) {
      throw new ValidationError('skillId', `Skill not found: ${skillId}`);
    }

    // Verify lifecycle state: personal skills can skip approval, team skills need 'approved'
    const canGenerate =
      (skill.status === 'proposed' && skill.scope === 'personal') ||
      skill.status === 'approved';

    if (!canGenerate) {
      throw new ValidationError('skillId',
        `Skill '${skill.name}' cannot be generated in status '${skill.status}' (scope: ${skill.scope}). ` +
        (skill.scope === 'team' ? 'Team skills must be approved first.' : 'Expected status: proposed.'));
    }

    // Build CommandSpec from SkillSpec
    const commandSpec: CommandSpec = {
      name: skill.spec.name,
      description: skill.spec.description,
      params: skill.spec.params.map(p => ({
        name: p.name,
        type: p.type,
        optional: p.optional,
        description: p.description,
      })),
      results: skill.spec.results.map(r => ({
        name: r.name,
        type: r.type,
        description: r.description,
      })),
      examples: skill.spec.examples?.map(e => ({
        description: e.description,
        command: e.command,
        expectedResult: e.expectedResult,
      })),
      accessLevel: skill.spec.accessLevel ?? 'ai-safe',
    };

    // Determine output directory
    const rootPath = path.resolve(__dirname, '../../../../');
    const outputDir = params.outputDir
      ?? (skill.scope === 'team'
        ? path.join(rootPath, 'commands', skill.spec.name)
        : path.join(rootPath, '.continuum', 'skills', skill.createdById, skill.spec.name));

    // Run CommandGenerator
    const generator = new CommandGenerator(rootPath);
    generator.generate(commandSpec, outputDir, { force: true });

    // Collect generated files
    const generatedFiles = this.collectFiles(outputDir);

    // Update entity
    await ORM.update<SkillEntity>(
      COLLECTIONS.SKILLS,
      skill.id as UUID,
      {
        status: 'generated',
        outputDir,
        generatedFiles,
      } as Partial<SkillEntity>,
    );

    return createSkillGenerateResultFromParams(params, {
      success: true,
      skillId: skill.id,
      name: skill.name,
      status: 'generated',
      outputDir,
      generatedFiles,
      message: `Generated ${generatedFiles.length} files for skill '${skill.name}' in ${outputDir}`,
    });
  }

  private collectFiles(dir: string): string[] {
    const files: string[] = [];
    if (!fs.existsSync(dir)) return files;

    const entries = fs.readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        files.push(...this.collectFiles(full));
      } else {
        files.push(full);
      }
    }
    return files;
  }
}
