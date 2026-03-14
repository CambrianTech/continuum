/**
 * dev/init — Detect project type and verify dev pipeline setup.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { DevInitParams, DevInitResult } from '../shared/DevInitTypes';
import { detectProject } from '../../../../system/sentinel/ProjectDetector';
import { TemplateRegistry } from '../../../../system/sentinel/pipelines/TemplateRegistry';

export class DevInitServerCommand extends CommandBase<DevInitParams, DevInitResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('dev/init', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<DevInitResult> {
    const typed = params as JTAGPayload & DevInitParams;
    const cwd = typed.repo || typed.cwd || process.cwd();

    const project = detectProject(cwd);
    const templates = TemplateRegistry.list().map(t => t.name);

    return transformPayload(params, {
      success: true,
      projectType: project.type,
      buildCommand: project.buildCommand,
      testCommand: project.testCommand,
      hasClaudeMd: project.hasClaudeMd,
      availableTemplates: templates,
    });
  }
}
