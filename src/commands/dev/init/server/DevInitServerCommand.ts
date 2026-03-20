/**
 * dev/init — Detect project type and verify dev pipeline setup.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { DevInitParams, DevInitResult } from '../shared/DevInitTypes';
import { ProjectDetector } from '../../../../system/code/server/ProjectDetector';
import { TemplateRegistry } from '../../../../system/sentinel/pipelines/TemplateRegistry';

export class DevInitServerCommand extends CommandBase<DevInitParams, DevInitResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('dev/init', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<DevInitResult> {
    const typed = params as JTAGPayload & DevInitParams;
    const cwd = typed.repo || typed.cwd || process.cwd();

    const project = await ProjectDetector.detect(cwd);
    const templates = TemplateRegistry.list().map(t => t.name);

    // Check for CLAUDE.md separately (canonical ProjectDetector doesn't track this)
    const fs = await import('fs');
    const path = await import('path');
    const hasClaudeMd = fs.existsSync(path.join(cwd, 'CLAUDE.md'));

    return transformPayload(params, {
      success: true,
      projectType: project.type,
      buildCommand: project.buildCommand ?? null,
      testCommand: project.testCommand ?? null,
      hasClaudeMd,
      availableTemplates: templates,
    });
  }
}
