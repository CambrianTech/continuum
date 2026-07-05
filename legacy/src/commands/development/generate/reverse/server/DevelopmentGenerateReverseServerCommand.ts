/**
 * Development Generate Reverse Command - Server Implementation
 *
 * Reverse-engineer a CommandSpec from an existing hand-written command.
 * Reads Types file, extracts params/results/name, outputs spec JSON.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { DevelopmentGenerateReverseParams, DevelopmentGenerateReverseResult } from '../shared/DevelopmentGenerateReverseTypes';
import { createDevelopmentGenerateReverseResultFromParams } from '../shared/DevelopmentGenerateReverseTypes';
import { CommandAuditor } from '@generator/CommandAuditor';
import * as fs from 'fs';
import * as path from 'path';

export class DevelopmentGenerateReverseServerCommand extends CommandBase<DevelopmentGenerateReverseParams, DevelopmentGenerateReverseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/generate/reverse', context, subpath, commander);
  }

  async execute(params: DevelopmentGenerateReverseParams): Promise<DevelopmentGenerateReverseResult> {
    if (!params.commandDir || params.commandDir.trim() === '') {
      throw new ValidationError(
        'commandDir',
        `Missing required parameter 'commandDir'. Pass a path like 'commands/ping' or 'commands/data/create'.`
      );
    }

    const auditor = new CommandAuditor(process.cwd());
    const spec = auditor.reverseEngineer(params.commandDir);

    if (!spec) {
      throw new ValidationError(
        'commandDir',
        `Could not reverse-engineer spec from '${params.commandDir}'. Ensure it has a shared/*Types.ts file.`
      );
    }

    const warnings: string[] = [];
    const specObj = spec as Record<string, unknown>;

    // Check for fields that need manual review
    if (specObj.description === 'TODO: Add description' || (specObj.description as string).length < 10) {
      warnings.push('Description was guessed from comments — review and improve it');
    }
    if (Array.isArray(specObj.examples) && specObj.examples.length === 1) {
      warnings.push('Only one generic example generated — add real usage examples');
    }

    let savedTo = '';
    if (params.save) {
      const name = (specObj.name as string).replace(/\//g, '-');
      const specPath = path.join(process.cwd(), 'generator', 'specs', `${name}.json`);
      fs.writeFileSync(specPath, JSON.stringify(spec, null, 2) + '\n', 'utf-8');
      savedTo = specPath;
    }

    return createDevelopmentGenerateReverseResultFromParams(params, {
      success: true,
      spec,
      savedTo,
      warnings
    });
  }
}
