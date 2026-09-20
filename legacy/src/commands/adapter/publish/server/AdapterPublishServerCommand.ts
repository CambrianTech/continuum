/**
 * Adapter Publish Command - Server Implementation
 *
 * Publishes a trained LoRA adapter to HuggingFace with auto-generated model card
 * and standardized continuum:* tags. Uses hf-publish.py for the actual upload
 * (huggingface_hub library). Supports create and update (versioned via HF commits).
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AdapterPublishParams, AdapterPublishResult } from '../shared/AdapterPublishTypes';
import { createAdapterPublishResultFromParams } from '../shared/AdapterPublishTypes';
import { DataRead } from '@commands/data/read/shared/DataReadTypes';
import { AcademySessionEntity } from '@system/genome/entities/AcademySessionEntity';
import { TeamProjectEntity } from '@system/genome/entities/TeamProjectEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export class AdapterPublishServerCommand extends CommandBase<AdapterPublishParams, AdapterPublishResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('adapter/publish', context, subpath, commander);
  }

  async execute(params: AdapterPublishParams): Promise<AdapterPublishResult> {
    if (!params.adapterPath) {
      throw new ValidationError('adapterPath', 'Required: path to adapter directory with manifest.json');
    }
    if (!params.repoId) {
      throw new ValidationError('repoId', 'Required: HuggingFace repo ID (e.g., "continuum-ai/my-adapter")');
    }

    const manifestPath = path.join(params.adapterPath, 'manifest.json');
    if (!fs.existsSync(manifestPath)) {
      throw new ValidationError('adapterPath', `No manifest.json found in ${params.adapterPath}`);
    }

    console.log(`📦 ADAPTER PUBLISH: ${params.repoId} from ${params.adapterPath}`);

    // Optionally export academy session data for model card
    let academyDataPath: string | undefined;
    if (params.academySessionId) {
      academyDataPath = await this.exportAcademyData(params.academySessionId as UUID, params.adapterPath);
    }

    // Optionally export team project data for model card
    let teamDataPath: string | undefined;
    if (params.teamProjectId) {
      teamDataPath = await this.exportTeamData(params.teamProjectId as UUID, params.adapterPath);
    }

    // Build Python command
    const scriptPath = path.resolve(__dirname, '../../../../system/genome/fine-tuning/server/adapters/scripts/hf-publish.py');
    const args = [
      'python3', scriptPath,
      '--adapter-path', params.adapterPath,
      '--repo-id', params.repoId,
    ];
    if (params.projectType) args.push('--project-type', params.projectType);
    if (academyDataPath) args.push('--academy-data', academyDataPath);
    if (teamDataPath) args.push('--team-data', teamDataPath);
    if (params.private) args.push('--private');
    if (params.update) args.push('--update');

    // Execute publish
    const cmd = args.map(a => a.includes(' ') ? `"${a}"` : a).join(' ');
    console.log(`   Running: ${cmd}`);

    const output = execSync(cmd, {
      encoding: 'utf-8',
      timeout: 300_000, // 5 minutes for upload
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    console.log(output);

    // Parse structured result from Python output
    const resultLine = output.split('\n').find(l => l.startsWith('PUBLISH_RESULT:'));
    if (resultLine) {
      const result = JSON.parse(resultLine.replace('PUBLISH_RESULT: ', ''));
      return createAdapterPublishResultFromParams(params, {
        success: true,
        repoUrl: result.repoUrl,
        tags: result.tags,
        modelCardGenerated: result.modelCardGenerated,
      });
    }

    return createAdapterPublishResultFromParams(params, {
      success: true,
      repoUrl: `https://huggingface.co/${params.repoId}`,
      tags: [],
      modelCardGenerated: true,
    });
  }

  /** Export academy session data as JSON for the model card generator */
  private async exportAcademyData(sessionId: UUID, adapterPath: string): Promise<string | undefined> {
    try {
      const result = await DataRead.execute({
        collection: AcademySessionEntity.collection,
        id: sessionId,
        dbHandle: 'default',
      });
      if (result.success && result.data) {
        const exportPath = path.join(adapterPath, '_academy_data.json');
        fs.writeFileSync(exportPath, JSON.stringify(result.data, null, 2));
        return exportPath;
      }
    } catch (e) {
      console.warn(`   Could not load academy session ${sessionId}: ${e}`);
    }
    return undefined;
  }

  /** Export team project data as JSON for the model card generator */
  private async exportTeamData(projectId: UUID, adapterPath: string): Promise<string | undefined> {
    try {
      const result = await DataRead.execute({
        collection: TeamProjectEntity.collection,
        id: projectId,
        dbHandle: 'default',
      });
      if (result.success && result.data) {
        const exportPath = path.join(adapterPath, '_team_data.json');
        fs.writeFileSync(exportPath, JSON.stringify(result.data, null, 2));
        return exportPath;
      }
    } catch (e) {
      console.warn(`   Could not load team project ${projectId}: ${e}`);
    }
    return undefined;
  }
}
