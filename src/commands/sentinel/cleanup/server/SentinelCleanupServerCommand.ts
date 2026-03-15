/**
 * Sentinel Cleanup — prune old sentinel logs, training datasets, and prompt captures.
 *
 * Data flows IN continuously (sentinel runs, training captures, prompt logs).
 * This command is the drain — removes data older than retention thresholds.
 *
 * Targets:
 * 1. ~/.continuum/jtag/logs/system/sentinels/{handle}/ — per-run pipeline logs
 * 2. ~/.continuum/datasets/*.jsonl — exported training data (consumed by genome/train)
 * 3. ~/.continuum/jtag/logs/prompt-captures.jsonl — full LLM request/response logs
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelCleanupParams, SentinelCleanupResult, CleanupStats } from '../shared/SentinelCleanupTypes';

export class SentinelCleanupServerCommand extends CommandBase<SentinelCleanupParams, SentinelCleanupResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/cleanup', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelCleanupResult> {
    const p = params as SentinelCleanupParams;
    const maxAgeHours = p.maxAgeHours ?? 72;       // 3 days for sentinel logs
    const datasetMaxAgeHours = p.datasetMaxAgeHours ?? 168; // 7 days for training data
    const dryRun = p.dryRun ?? false;
    const cleanPromptCaptures = p.cleanPromptCaptures ?? true;
    const cleanAdapters = p.cleanAdapters ?? true;
    const adapterMaxAgeHours = p.adapterMaxAgeHours ?? 336; // 14 days

    const home = process.env.HOME || '/tmp';
    const now = Date.now();

    const deleted: CleanupStats = { sentinelDirs: 0, sentinelBytes: 0, datasetFiles: 0, datasetBytes: 0, promptCaptureBytes: 0, adapterDirs: 0, adapterBytes: 0 };
    const remaining: CleanupStats = { sentinelDirs: 0, sentinelBytes: 0, datasetFiles: 0, datasetBytes: 0, promptCaptureBytes: 0, adapterDirs: 0, adapterBytes: 0 };

    try {
      // 1. Sentinel log directories
      const sentinelsDir = path.join(home, '.continuum', 'jtag', 'logs', 'system', 'sentinels');
      if (fs.existsSync(sentinelsDir)) {
        const handles = fs.readdirSync(sentinelsDir);
        for (const handle of handles) {
          const handleDir = path.join(sentinelsDir, handle);
          const stat = fs.statSync(handleDir);
          if (!stat.isDirectory()) continue;

          const ageHours = (now - stat.mtimeMs) / (1000 * 60 * 60);
          const dirSize = this.getDirSize(handleDir);

          if (ageHours > maxAgeHours) {
            deleted.sentinelDirs++;
            deleted.sentinelBytes += dirSize;
            if (!dryRun) {
              fs.rmSync(handleDir, { recursive: true, force: true });
            }
          } else {
            remaining.sentinelDirs++;
            remaining.sentinelBytes += dirSize;
          }
        }
      }

      // 2. Training datasets — JSONL files AND intermediate subdirectories
      //    Subdirs like prepared/, realclasseval-raw/ accumulate GB of stale data.
      //    The active dataset (realclasseval/) is small; intermediates are the problem.
      const datasetsDir = path.join(home, '.continuum', 'datasets');
      if (fs.existsSync(datasetsDir)) {
        const entries = fs.readdirSync(datasetsDir);
        for (const entry of entries) {
          const entryPath = path.join(datasetsDir, entry);
          const stat = fs.statSync(entryPath);
          const ageHours = (now - stat.mtimeMs) / (1000 * 60 * 60);

          if (stat.isFile() && entry.endsWith('.jsonl')) {
            if (ageHours > datasetMaxAgeHours) {
              deleted.datasetFiles++;
              deleted.datasetBytes += stat.size;
              if (!dryRun) fs.unlinkSync(entryPath);
            } else {
              remaining.datasetFiles++;
              remaining.datasetBytes += stat.size;
            }
          } else if (stat.isDirectory()) {
            // Prune intermediate directories older than retention (e.g., prepared/, *-raw/)
            const dirSize = this.getDirSize(entryPath);
            if (ageHours > datasetMaxAgeHours) {
              deleted.datasetFiles++;
              deleted.datasetBytes += dirSize;
              if (!dryRun) fs.rmSync(entryPath, { recursive: true, force: true });
            } else {
              remaining.datasetFiles++;
              remaining.datasetBytes += dirSize;
            }
          }
        }
      }

      // 3. Prompt capture log (single file, can grow huge)
      if (cleanPromptCaptures) {
        const promptCapturePath = path.join(home, '.continuum', 'jtag', 'logs', 'prompt-captures.jsonl');
        if (fs.existsSync(promptCapturePath)) {
          const stat = fs.statSync(promptCapturePath);
          // Truncate if over 50MB or older than retention
          const ageHours = (now - stat.mtimeMs) / (1000 * 60 * 60);
          const MAX_PROMPT_CAPTURE_BYTES = 50 * 1024 * 1024; // 50MB

          if (stat.size > MAX_PROMPT_CAPTURE_BYTES || ageHours > maxAgeHours) {
            deleted.promptCaptureBytes = stat.size;
            if (!dryRun) {
              // Keep last 100 lines max, and enforce 10MB cap on the kept content.
              // Each line is a full LLM req/res (~100KB), so 100 lines ≈ 10MB.
              const content = fs.readFileSync(promptCapturePath, 'utf-8');
              const lines = content.split('\n');
              let kept = lines.slice(-100).join('\n');
              const MAX_KEPT_BYTES = 10 * 1024 * 1024; // 10MB
              if (Buffer.byteLength(kept) > MAX_KEPT_BYTES) {
                // Still too big — keep fewer lines
                const reducedLines = lines.slice(-20).join('\n');
                kept = reducedLines;
              }
              fs.writeFileSync(promptCapturePath, kept, 'utf-8');
              remaining.promptCaptureBytes = Buffer.byteLength(kept);
            }
          } else {
            remaining.promptCaptureBytes = stat.size;
          }
        }
      }

      // 4. LoRA adapter directories — prune old checkpoints and stale adapters
      if (cleanAdapters) {
        const adaptersDir = path.join(home, '.continuum', 'genome', 'adapters');
        if (fs.existsSync(adaptersDir)) {
          const adapterDirs = fs.readdirSync(adaptersDir);
          for (const adapterDir of adapterDirs) {
            const adapterPath = path.join(adaptersDir, adapterDir);
            const stat = fs.statSync(adapterPath);
            if (!stat.isDirectory()) continue;

            const ageHours = (now - stat.mtimeMs) / (1000 * 60 * 60);

            // First: always clean intermediate checkpoints (checkpoint-N/) inside any adapter
            const subEntries = fs.readdirSync(adapterPath);
            for (const sub of subEntries) {
              if (sub.startsWith('checkpoint-')) {
                const checkpointPath = path.join(adapterPath, sub);
                const checkpointStat = fs.statSync(checkpointPath);
                if (checkpointStat.isDirectory()) {
                  const checkpointSize = this.getDirSize(checkpointPath);
                  const checkpointAge = (now - checkpointStat.mtimeMs) / (1000 * 60 * 60);
                  // Delete checkpoints older than 3 days (they're intermediate, final is adapter_model.safetensors)
                  if (checkpointAge > 72) {
                    deleted.adapterDirs++;
                    deleted.adapterBytes += checkpointSize;
                    if (!dryRun) fs.rmSync(checkpointPath, { recursive: true, force: true });
                  }
                }
              }
            }

            // Second: delete entire adapter dir if older than retention threshold
            if (ageHours > adapterMaxAgeHours) {
              const dirSize = this.getDirSize(adapterPath);
              deleted.adapterDirs++;
              deleted.adapterBytes += dirSize;
              if (!dryRun) fs.rmSync(adapterPath, { recursive: true, force: true });
            } else {
              remaining.adapterDirs++;
              remaining.adapterBytes += this.getDirSize(adapterPath);
            }
          }
        }
      }

      const mode = dryRun ? ' (dry run)' : '';
      console.log(`🧹 Sentinel cleanup${mode}: ${deleted.sentinelDirs} sentinel dirs (${this.formatBytes(deleted.sentinelBytes)}), ${deleted.datasetFiles} datasets (${this.formatBytes(deleted.datasetBytes)}), ${deleted.adapterDirs} adapters (${this.formatBytes(deleted.adapterBytes)}), prompt: ${this.formatBytes(deleted.promptCaptureBytes)}`);

      return transformPayload(params, {
        success: true,
        deleted,
        remaining,
      });
    } catch (error) {
      return transformPayload(params, {
        success: false,
        deleted,
        remaining,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  private getDirSize(dirPath: string): number {
    let total = 0;
    try {
      const entries = fs.readdirSync(dirPath);
      for (const entry of entries) {
        const entryPath = path.join(dirPath, entry);
        const stat = fs.statSync(entryPath);
        if (stat.isFile()) {
          total += stat.size;
        } else if (stat.isDirectory()) {
          total += this.getDirSize(entryPath);
        }
      }
    } catch {
      // Permission error or race condition — skip
    }
    return total;
  }

  private formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes}B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)}GB`;
  }
}
