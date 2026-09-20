/**
 * Model Download Command - Server Implementation
 *
 * Download a base model from HuggingFace. Can target a remote grid node
 * for large models that need GPU VRAM. Uses huggingface_hub snapshot_download.
 */

import { execFileSync } from 'child_process';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { ModelDownloadParams, ModelDownloadResult } from '../shared/ModelDownloadTypes';
import { createModelDownloadResultFromParams } from '../shared/ModelDownloadTypes';

const pythonLiteral = (value: string | undefined): string => value === undefined ? 'None' : JSON.stringify(value);

export class ModelDownloadServerCommand extends CommandBase<ModelDownloadParams, ModelDownloadResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/download', context, subpath, commander);
  }

  async execute(params: ModelDownloadParams): Promise<ModelDownloadResult> {
    if (!params.modelId) {
      throw new ValidationError('modelId', 'Required: HuggingFace model ID (e.g., "Qwen/Qwen3.5-27B")');
    }

    const modelId = params.modelId;
    const node = params.node as string | undefined;
    const revision = params.revision as string | undefined;

    console.log(`📥 MODEL DOWNLOAD: ${modelId}${node ? ` → ${node}` : ' (local)'}`);

    const revisionLiteral = pythonLiteral(revision);
    const pythonScript = `
from huggingface_hub import snapshot_download
import json, os
kwargs = {}
revision = ${revisionLiteral}
if revision is not None:
    kwargs["revision"] = revision
path = snapshot_download(${JSON.stringify(modelId)}, **kwargs)
size = sum(os.path.getsize(os.path.join(dp, f)) for dp, _, fns in os.walk(path) for f in fns)
print(json.dumps({'path': path, 'sizeGb': round(size / 1e9, 2)}))
`;

    try {
      let output: string;

      if (node) {
        // Download on remote node via SSH
        console.log(`   Downloading on remote node ${node}...`);
        const sshUser = process.env.CONTINUUM_SSH_USER ?? process.env.USER ?? process.env.LOGNAME;
        if (!sshUser) {
          throw new Error('CONTINUUM_SSH_USER or USER must be set for remote model download');
        }
        output = execFileSync(
          'ssh',
          [
            '-o',
            'ConnectTimeout=10',
            '-o',
            'StrictHostKeyChecking=no',
            `${sshUser}@${node}`,
            'python3',
            '-c',
            pythonScript,
          ],
          { encoding: 'utf-8', timeout: 3600_000 }, // 1 hour timeout for large models
        ).trim();
      } else {
        // Download locally
        console.log('   Downloading locally...');
        output = execFileSync('python3', ['-c', pythonScript], {
          encoding: 'utf-8',
          timeout: 3600_000,
        }).trim();
      }

      // Parse the JSON output line (last line)
      const lines = output.split('\n');
      const jsonLine = [...lines].reverse().find((l: string) => l.startsWith('{'));
      if (!jsonLine) {
        throw new Error('No JSON output from download script');
      }

      const result = JSON.parse(jsonLine) as { path: string; sizeGb: number };
      console.log(`✅ Downloaded: ${result.path} (${result.sizeGb} GB)`);

      return createModelDownloadResultFromParams(params, {
        success: true,
        downloadPath: result.path,
        sizeGb: result.sizeGb,
        nodeId: node ?? 'local',
      });
    } catch (e) {
      throw new Error(`Download failed: ${e instanceof Error ? e.message : e}`);
    }
  }
}
