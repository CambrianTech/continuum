/**
 * Model Publish Command - Server Implementation
 *
 * Publish a forged model to HuggingFace — safetensors, config, tokenizer,
 * model card, and alloy provenance. This is the Factory's shipping department.
 *
 * Uses huggingface_hub Python API via subprocess (same pattern as adapter/publish).
 * Requires HF_TOKEN in the environment or ~/.huggingface/token.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { ModelPublishParams, ModelPublishResult } from '../shared/ModelPublishTypes';
import { createModelPublishResultFromParams } from '../shared/ModelPublishTypes';
import { Commands } from '@system/core/shared/Commands';
import type { GridSendParams, GridSendResult } from '@commands/grid/send/shared/GridSendTypes';
import { spawn } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';

export class ModelPublishServerCommand extends CommandBase<ModelPublishParams, ModelPublishResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/publish', context, subpath, commander);
  }

  async execute(params: ModelPublishParams): Promise<ModelPublishResult> {
    const forgedDir = params.forgedDir;
    const repoName = params.repoName;
    const org = params.org || 'continuum-ai';
    const repoId = `${org}/${repoName}`;

    // Validate required params
    if (!forgedDir || forgedDir.trim() === '') {
      throw new ValidationError(
        'forgedDir',
        `Missing required parameter 'forgedDir'. Path to the forged model directory.`
      );
    }
    if (!repoName || repoName.trim() === '') {
      throw new ValidationError(
        'repoName',
        `Missing required parameter 'repoName'. HuggingFace repo name (e.g., 'mixtral-8x7b-instruct-compacted-conservative').`
      );
    }

    // If nodeId specified, delegate to the remote node via grid/send
    if (params.nodeId) {
      return this.executeRemote(params);
    }

    // Local publish — forgedDir must exist on this machine
    if (!fs.existsSync(forgedDir)) {
      throw new ValidationError(
        'forgedDir',
        `Forged directory '${forgedDir}' does not exist on this machine. ` +
        `If the model is on a grid node, pass nodeId to execute remotely.`
      );
    }

    // Find model files
    const modelFiles = this.findModelFiles(forgedDir);
    if (modelFiles.safetensors.length === 0) {
      throw new ValidationError(
        'forgedDir',
        `No .safetensors files found in '${forgedDir}'. Is this a valid forged model directory?`
      );
    }

    // Find or use provided card
    let cardPath = params.cardPath;
    if (!cardPath) {
      // Look for README.md in the forged dir
      const readmePath = path.join(forgedDir, 'README.md');
      if (fs.existsSync(readmePath)) {
        cardPath = readmePath;
      }
    }

    // Find alloy
    let alloyPath = params.alloyPath;
    if (!alloyPath) {
      const alloyFiles = fs.readdirSync(forgedDir).filter(f => f.endsWith('.alloy.json'));
      if (alloyFiles.length > 0) {
        alloyPath = path.join(forgedDir, alloyFiles[0]);
      }
    }

    console.log(`📦 Publishing model to ${repoId}`);
    console.log(`   forgedDir: ${forgedDir}`);
    console.log(`   files: ${modelFiles.all.length} (${modelFiles.safetensors.length} safetensors)`);
    console.log(`   card: ${cardPath || 'none'}`);
    console.log(`   alloy: ${alloyPath || 'none'}`);

    // Build the file list for upload
    const filesToUpload: string[] = [...modelFiles.all];
    if (alloyPath && !filesToUpload.includes(alloyPath)) {
      filesToUpload.push(alloyPath);
    }

    // Calculate total size
    let totalBytes = 0;
    for (const f of filesToUpload) {
      try { totalBytes += fs.statSync(f).size; } catch { /* skip */ }
    }
    const totalSizeGb = Math.round(totalBytes / 1e9 * 100) / 100;

    // Execute HF upload via Python subprocess
    try {
      const result = await this.runHfUpload({
        repoId,
        forgedDir,
        cardPath,
        alloyPath,
        isPrivate: params.private || false,
        filesToUpload,
      });

      console.log(`✅ Published to https://huggingface.co/${repoId}`);

      return createModelPublishResultFromParams(params, {
        success: true,
        repoUrl: `https://huggingface.co/${repoId}`,
        repoId,
        filesUploaded: filesToUpload.length + (cardPath ? 1 : 0),
        totalSizeGb,
        cardIncluded: !!cardPath,
        alloyIncluded: !!alloyPath,
      });

    } catch (err: any) {
      console.error(`❌ Publish failed:`, err.message || err);
      return createModelPublishResultFromParams(params, {
        success: false,
        repoUrl: '',
        repoId,
        filesUploaded: 0,
        totalSizeGb: 0,
        cardIncluded: false,
        alloyIncluded: false,
        error: { message: err.message || String(err), code: 'PUBLISH_FAILED' } as any,
      });
    }
  }

  /**
   * Find all publishable files in the forged directory.
   */
  private findModelFiles(dir: string): { safetensors: string[]; all: string[] } {
    const publishableExtensions = [
      '.safetensors', '.json', '.txt', '.model', '.py',
      '.gguf', '.bin', '.alloy.json', '.png',
    ];
    const excludePatterns = ['status.json', 'REGRESSION_HALT.json'];

    const all: string[] = [];
    const safetensors: string[] = [];

    // Check forgedDir and forgedDir/model/ (the alloy_executor convention)
    const dirs = [dir];
    const modelSubdir = path.join(dir, 'model');
    if (fs.existsSync(modelSubdir)) {
      dirs.push(modelSubdir);
    }
    // Also check forgedDir/pruned/ (the expert-prune output convention)
    const prunedSubdir = path.join(dir, 'pruned');
    if (fs.existsSync(prunedSubdir)) {
      dirs.push(prunedSubdir);
    }

    for (const d of dirs) {
      for (const file of fs.readdirSync(d)) {
        const fullPath = path.join(d, file);
        if (!fs.statSync(fullPath).isFile()) continue;
        if (excludePatterns.some(p => file === p)) continue;
        if (publishableExtensions.some(ext => file.endsWith(ext))) {
          all.push(fullPath);
          if (file.endsWith('.safetensors')) {
            safetensors.push(fullPath);
          }
        }
      }
    }

    return { safetensors, all };
  }

  /**
   * Upload to HuggingFace via huggingface_hub Python API.
   *
   * Uses a subprocess because huggingface_hub is Python-only. Same pattern
   * as adapter/publish's hf-publish.py. The subprocess handles auth via
   * HF_TOKEN env var or ~/.huggingface/token.
   */
  private runHfUpload(opts: {
    repoId: string;
    forgedDir: string;
    cardPath: string | undefined;
    alloyPath: string | undefined;
    isPrivate: boolean;
    filesToUpload: string[];
  }): Promise<{ url: string }> {
    return new Promise((resolve, reject) => {
      const script = `
import os, sys, json
from huggingface_hub import HfApi, create_repo
from pathlib import Path

repo_id = ${JSON.stringify(opts.repoId)}
forged_dir = ${JSON.stringify(opts.forgedDir)}
card_path = ${JSON.stringify(opts.cardPath || '')}
is_private = ${opts.isPrivate ? 'True' : 'False'}

api = HfApi()

# Create or get the repo
try:
    create_repo(repo_id, repo_type="model", private=is_private, exist_ok=True)
    print(f"repo: {repo_id}", file=sys.stderr)
except Exception as e:
    print(f"repo create warning: {e}", file=sys.stderr)

# Upload the card first (if provided)
if card_path and os.path.exists(card_path):
    api.upload_file(
        path_or_fileobj=card_path,
        path_in_repo="README.md",
        repo_id=repo_id,
        repo_type="model",
    )
    print(f"uploaded: README.md", file=sys.stderr)

# Upload all model files from the forged directory
# Check model/ subdir first (alloy_executor convention), then pruned/, then root
upload_dirs = []
model_sub = Path(forged_dir) / "model"
pruned_sub = Path(forged_dir) / "pruned"
if model_sub.exists():
    upload_dirs.append(("model", model_sub))
if pruned_sub.exists():
    upload_dirs.append(("pruned", pruned_sub))
upload_dirs.append(("root", Path(forged_dir)))

uploaded = 0
for label, d in upload_dirs:
    for f in sorted(d.iterdir()):
        if not f.is_file():
            continue
        if f.name in ("status.json", "REGRESSION_HALT.json"):
            continue
        if f.suffix in (".safetensors", ".json", ".txt", ".model", ".gguf", ".png"):
            api.upload_file(
                path_or_fileobj=str(f),
                path_in_repo=f.name,
                repo_id=repo_id,
                repo_type="model",
            )
            print(f"uploaded: {f.name} ({f.stat().st_size / 1e9:.2f} GB)", file=sys.stderr)
            uploaded += 1

# Upload alloy if present and not already uploaded
alloy_files = list(Path(forged_dir).glob("*.alloy.json"))
for af in alloy_files:
    api.upload_file(
        path_or_fileobj=str(af),
        path_in_repo=af.name,
        repo_id=repo_id,
        repo_type="model",
    )
    print(f"uploaded: {af.name} (alloy)", file=sys.stderr)
    uploaded += 1

result = {"url": f"https://huggingface.co/{repo_id}", "uploaded": uploaded}
print(json.dumps(result))
`;

      // Find Python — prefer the genome venv, fall back to system
      const genomePython = path.resolve(__dirname, '../../../../system/genome/python/venv/bin/python');
      const pythonPath = fs.existsSync(genomePython) ? genomePython : 'python3';

      const proc = spawn(pythonPath, ['-c', script], {
        env: { ...process.env },
        stdio: ['pipe', 'pipe', 'pipe'],
      });

      let stdout = '';
      let stderr = '';

      proc.stdout.on('data', (data: Buffer) => { stdout += data.toString(); });
      proc.stderr.on('data', (data: Buffer) => {
        stderr += data.toString();
        // Log upload progress in real time
        const lines = data.toString().split('\n').filter((l: string) => l.trim());
        for (const line of lines) {
          console.log(`   [hf] ${line}`);
        }
      });

      proc.on('close', (code: number) => {
        if (code !== 0) {
          reject(new Error(`HF upload failed (exit ${code}): ${stderr}`));
          return;
        }
        try {
          const result = JSON.parse(stdout.trim());
          resolve(result);
        } catch {
          reject(new Error(`HF upload produced invalid JSON: ${stdout}`));
        }
      });
    });
  }

  /**
   * Execute publish on a remote grid node via grid/send.
   */
  private async executeRemote(params: ModelPublishParams): Promise<ModelPublishResult> {
    // Delegate to grid/send which executes model/publish on the remote node.
    const remoteParams = { ...params, nodeId: undefined };
    const gridResult = await Commands.execute<GridSendParams, GridSendResult>(
      'grid/send',
      {
        ...params, // inherits context, sessionId, userId from the caller
        nodeId: params.nodeId!,
        remoteCommand: 'model/publish',
        params: remoteParams as unknown as Record<string, unknown>,
      },
    );

    if (gridResult?.success && gridResult?.remoteResult) {
      return gridResult.remoteResult as ModelPublishResult;
    }

    return createModelPublishResultFromParams(params, {
      success: false,
      repoUrl: '',
      repoId: `${params.org || 'continuum-ai'}/${params.repoName}`,
      filesUploaded: 0,
      totalSizeGb: 0,
      cardIncluded: false,
      alloyIncluded: false,
      error: { message: `Remote publish failed on node ${params.nodeId}`, code: 'REMOTE_FAILED' } as any,
    });
  }
}
