/**
 * Sentinel Run Command - Server Implementation
 *
 * Creates and runs Sentinels from JSON config.
 * Uses handles for long-running operations, emits events for progress.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Events } from '../../../../system/core/shared/Events';
import { v4 as uuid } from 'uuid';
import type {
  SentinelRunParams,
  SentinelRunResult,
  SentinelResultData,
  AnySentinelParams,
  BuildSentinelParams,
  OrchestrateSentinelParams,
  ScreenshotSentinelParams,
  TaskSentinelParams,
  SentinelType,
} from '../shared/SentinelRunTypes';

// Import sentinels
import { BuildSentinel } from '../../../../system/sentinel/BuildSentinel';
import { OrchestratorSentinel } from '../../../../system/sentinel/OrchestratorSentinel';
import { VisualSentinel } from '../../../../system/sentinel/VisualSentinel';
import { ModelCapacity, ModelProvider } from '../../../../system/sentinel/ModelProvider';

/**
 * Active sentinel handles
 */
interface SentinelHandle {
  id: string;
  type: SentinelType;
  status: 'running' | 'completed' | 'failed';
  progress: number;
  startTime: number;
  userId?: string;  // Who initiated this sentinel
  data?: SentinelResultData['data'];
  error?: string;
}

const activeHandles = new Map<string, SentinelHandle>();

export class SentinelRunServerCommand extends CommandBase<SentinelRunParams, SentinelRunResult> {
  private workingDir: string;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/run', context, subpath, commander);
    this.workingDir = process.cwd();
  }

  async execute(params: JTAGPayload): Promise<SentinelRunResult> {
    let sentinelParams = params as AnySentinelParams;

    // Parse tasks if passed as JSON string (from CLI)
    if (sentinelParams.type === 'task' && typeof (sentinelParams as any).tasks === 'string') {
      try {
        (sentinelParams as any).tasks = JSON.parse((sentinelParams as any).tasks);
      } catch (e) {
        return transformPayload(params, {
          success: false,
          completed: true,
          data: { success: false, errors: ['Invalid tasks JSON: ' + (e as Error).message] },
        });
      }
    }

    const workingDir = sentinelParams.workingDir || this.workingDir;
    const runAsync = sentinelParams.async !== false;  // Default to async

    // Create handle for tracking
    const handle = uuid().slice(0, 8);
    const userId = (params as any).userId as string | undefined;
    const handleData: SentinelHandle = {
      id: handle,
      type: sentinelParams.type,
      status: 'running',
      progress: 0,
      userId,
      startTime: Date.now(),
    };
    activeHandles.set(handle, handleData);

    // Emit start event
    this.emitProgress(handle, sentinelParams.type, 'starting', 0, `Starting ${sentinelParams.type} sentinel`);

    // Run sentinel based on type
    if (runAsync) {
      // Run async - return handle immediately
      this.runSentinelAsync(handle, sentinelParams, workingDir);
      return transformPayload(params, {
        success: true,
        handle,
        completed: false,
      });
    } else {
      // Run sync - wait for completion
      const result = await this.runSentinel(handle, sentinelParams, workingDir);
      return transformPayload(params, result);
    }
  }

  /**
   * Run sentinel asynchronously
   */
  private async runSentinelAsync(handle: string, params: AnySentinelParams, workingDir: string): Promise<void> {
    try {
      const result = await this.runSentinel(handle, params, workingDir);

      const handleData = activeHandles.get(handle);
      if (handleData) {
        handleData.status = result.data?.success ? 'completed' : 'failed';
        handleData.progress = 100;
        handleData.data = result.data;
      }

      // Emit complete event (include userId for memory capture)
      Events.emit('sentinel:complete', {
        handle,
        type: params.type,
        userId: handleData?.userId,
        success: result.data?.success || false,
        data: result.data,
      });
    } catch (error: any) {
      const handleData = activeHandles.get(handle);
      if (handleData) {
        handleData.status = 'failed';
        handleData.error = error.message;
      }

      Events.emit('sentinel:error', {
        handle,
        type: params.type,
        userId: handleData?.userId,
        error: error.message,
      });
    }
  }

  /**
   * Run sentinel and return result
   */
  private async runSentinel(handle: string, params: AnySentinelParams, workingDir: string): Promise<SentinelResultData> {
    switch (params.type) {
      case 'build':
        return this.runBuildSentinel(handle, params as BuildSentinelParams, workingDir);
      case 'orchestrate':
        return this.runOrchestrateSentinel(handle, params as OrchestrateSentinelParams, workingDir);
      case 'screenshot':
        return this.runScreenshotSentinel(handle, params as ScreenshotSentinelParams, workingDir);
      case 'task':
        return this.runTaskSentinel(handle, params as TaskSentinelParams, workingDir);
      default:
        return {
          success: false,
          completed: true,
          data: { success: false, errors: [`Unknown sentinel type: ${(params as any).type}`] },
        };
    }
  }

  /**
   * Run BuildSentinel
   */
  private async runBuildSentinel(handle: string, params: BuildSentinelParams, workingDir: string): Promise<SentinelResultData> {
    const sentinel = new BuildSentinel({
      command: params.command,
      workingDir,
      maxAttempts: params.maxAttempts || 5,
      canAutoFix: params.canAutoFix !== false,
      // LLM-assisted error fixing
      useLLM: params.useLLM,
      capacity: params.capacity as ModelCapacity,
      provider: params.provider as ModelProvider,
      onProgress: (progress) => {
        this.emitProgress(handle, 'build', progress.phase, progress.attempt / (params.maxAttempts || 5) * 100, progress.message);
      },
    });

    const result = await sentinel.run();

    return {
      success: result.success,
      handle,
      completed: true,
      data: {
        success: result.success,
        summary: result.success ? 'Build succeeded' : result.escalationReason,
        attempts: result.attempts.length,
        errors: result.attempts.filter(a => !a.success).map(a => a.errors.map(e => e.message)).flat(),
      },
    };
  }

  /**
   * Run OrchestratorSentinel
   */
  private async runOrchestrateSentinel(handle: string, params: OrchestrateSentinelParams, workingDir: string): Promise<SentinelResultData> {
    const sentinel = new OrchestratorSentinel({
      workingDir,
      maxIterations: params.maxIterations || 10,
      capacity: params.capacity as ModelCapacity || ModelCapacity.SMALL,
      provider: params.provider as ModelProvider || ModelProvider.LOCAL,
      modelName: params.modelName,
      screenshotDir: params.screenshotDir || '/tmp/sentinel-screenshots',
      onThought: (thought) => {
        this.emitProgress(handle, 'orchestrate', 'thinking', 50, thought);
      },
      onAction: (action, result) => {
        this.emitProgress(handle, 'orchestrate', 'acting', 75, `${action}: ${result.slice(0, 100)}`);
      },
      onScreenshot: (path) => {
        Events.emit('sentinel:screenshot', { handle, type: 'orchestrate', path });
      },
    });

    const result = await sentinel.execute(params.goal);

    return {
      success: result.success,
      handle,
      completed: true,
      data: {
        success: result.success,
        summary: result.summary,
        filesCreated: result.context.filesCreated,
        filesModified: result.context.filesModified,
        errors: result.context.errors,
        iterations: result.context.iteration,
      },
    };
  }

  /**
   * Run VisualSentinel
   */
  private async runScreenshotSentinel(handle: string, params: ScreenshotSentinelParams, workingDir: string): Promise<SentinelResultData> {
    const sentinel = new VisualSentinel({
      outputDir: params.outputDir || '/tmp/sentinel-screenshots',
      viewport: params.viewport,
    });

    this.emitProgress(handle, 'screenshot', 'capturing', 50, `Taking screenshot of ${params.target}`);

    let result;
    if (params.target.startsWith('http://') || params.target.startsWith('https://')) {
      result = await sentinel.screenshotUrl(params.target, params.filename);
    } else {
      result = await sentinel.screenshotFile(params.target, params.filename);
    }

    if (result.success && result.imagePath) {
      Events.emit('sentinel:screenshot', { handle, type: 'screenshot', path: result.imagePath });
    }

    return {
      success: result.success,
      handle,
      completed: true,
      data: {
        success: result.success,
        screenshot: result.imagePath,
        errors: result.error ? [result.error] : undefined,
      },
    };
  }

  /**
   * Run TaskSentinel (simplified - just execute tasks in order)
   */
  private async runTaskSentinel(handle: string, params: TaskSentinelParams, workingDir: string): Promise<SentinelResultData> {
    const results: Array<{ name: string; success: boolean; output: string }> = [];

    for (let i = 0; i < params.tasks.length; i++) {
      const task = params.tasks[i];
      this.emitProgress(handle, 'task', task.name, (i / params.tasks.length) * 100, `Executing ${task.name}`);

      try {
        let taskResult: { success: boolean; output: string };

        switch (task.action) {
          case 'write':
            if (task.file && task.content) {
              const fs = await import('fs');
              const path = await import('path');
              const fullPath = path.resolve(workingDir, task.file);
              const dir = path.dirname(fullPath);
              if (!fs.existsSync(dir)) {
                fs.mkdirSync(dir, { recursive: true });
              }
              fs.writeFileSync(fullPath, task.content);
              Events.emit('sentinel:file:created', { handle, type: 'task', path: fullPath, size: task.content.length });
              taskResult = { success: true, output: `Wrote ${task.file}` };
            } else {
              taskResult = { success: false, output: 'Write requires file and content' };
            }
            break;

          case 'read':
            if (task.file) {
              const fs = await import('fs');
              const path = await import('path');
              const content = fs.readFileSync(path.resolve(workingDir, task.file), 'utf-8');
              taskResult = { success: true, output: content.slice(0, 500) };
            } else {
              taskResult = { success: false, output: 'Read requires file' };
            }
            break;

          case 'run':
            if (task.command) {
              const { execSync } = await import('child_process');
              try {
                const output = execSync(task.command, { cwd: workingDir, encoding: 'utf-8', timeout: 30000 });
                taskResult = { success: true, output: output.slice(0, 500) };
              } catch (e: any) {
                taskResult = { success: false, output: e.message };
              }
            } else {
              taskResult = { success: false, output: 'Run requires command' };
            }
            break;

          case 'build':
            const buildResult = await new BuildSentinel({
              command: task.command || 'npm run build',
              workingDir,
              maxAttempts: 3,
              canAutoFix: true,
            }).run();
            taskResult = { success: buildResult.success, output: buildResult.escalationReason || 'Build complete' };
            break;

          default:
            taskResult = { success: false, output: `Unknown action: ${task.action}` };
        }

        results.push({ name: task.name, ...taskResult });
      } catch (error: any) {
        results.push({ name: task.name, success: false, output: error.message });
      }
    }

    const allSuccess = results.every(r => r.success);

    return {
      success: allSuccess,
      handle,
      completed: true,
      data: {
        success: allSuccess,
        summary: `Completed ${results.length} tasks, ${results.filter(r => r.success).length} succeeded`,
        errors: results.filter(r => !r.success).map(r => `${r.name}: ${r.output}`),
      },
    };
  }

  /**
   * Emit progress event
   */
  private emitProgress(handle: string, type: SentinelType, step: string, progress: number, message: string): void {
    const handleData = activeHandles.get(handle);
    if (handleData) {
      handleData.progress = progress;
    }

    Events.emit('sentinel:progress', { handle, type, userId: handleData?.userId, step, progress, message });
  }
}

/**
 * Get handle owner (userId)
 */
export function getHandleOwner(handle: string): string | undefined {
  return activeHandles.get(handle)?.userId;
}

/**
 * Get status of a sentinel handle
 */
export function getSentinelStatus(handle: string): SentinelHandle | undefined {
  return activeHandles.get(handle);
}

/**
 * Clear old handles (cleanup)
 */
export function cleanupHandles(maxAgeMs: number = 3600000): void {
  const now = Date.now();
  for (const [id, handle] of activeHandles) {
    if (now - handle.startTime > maxAgeMs && handle.status !== 'running') {
      activeHandles.delete(id);
    }
  }
}
