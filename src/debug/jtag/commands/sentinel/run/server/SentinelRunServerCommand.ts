/**
 * Sentinel Run Command - Server Implementation
 *
 * Fire-and-forget wrapper that forwards to Rust SentinelModule.
 * Returns handle immediately. Status via sentinel/status.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelRunParams, SentinelRunResult } from '../shared/SentinelRunTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { Pipeline } from '../../../../workers/continuum-core/bindings/modules/sentinel';
import { registerSentinelHandle } from '../../../../system/sentinel/SentinelEscalationService';

export class SentinelRunServerCommand extends CommandBase<SentinelRunParams, SentinelRunResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/run', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelRunResult> {
    // Parse definition
    let definition: any;

    if ((params as any).definition) {
      definition = typeof (params as any).definition === 'string'
        ? JSON.parse((params as any).definition)
        : (params as any).definition;
    } else if ((params as any).steps) {
      definition = { steps: (params as any).steps };
    } else {
      return transformPayload(params, {
        success: false,
        completed: true,
        error: 'Missing pipeline definition. Provide --definition or --steps',
      });
    }

    const workingDir = (params as any).workingDir || process.cwd();
    const asyncMode = (params as SentinelRunParams).async !== false; // Default: async (fire-and-forget)

    // Build pipeline for Rust
    const pipeline: Pipeline = {
      name: definition.name || 'unnamed',
      steps: definition.steps,
      workingDir,
      timeoutSecs: definition.timeoutSecs || definition.timeout_secs,
      inputs: definition.inputs || {},
    };

    const rustClient = RustCoreIPCClient.getInstance();
    const sentinelRunParams = {
      type: 'pipeline',
      command: 'pipeline',
      args: [] as string[],
      workingDir,
      env: { PIPELINE_JSON: JSON.stringify(pipeline) },
      timeout: pipeline.timeoutSecs,
    };

    try {
      if (asyncMode) {
        // Fire-and-forget: return handle immediately
        const result = await rustClient.sentinelRun(sentinelRunParams);

        // Register handle for lifecycle tracking (escalation → persona inbox)
        const runParams = params as SentinelRunParams;
        if (result.handle && (runParams.entityId || runParams.parentPersonaId)) {
          registerSentinelHandle(
            result.handle,
            runParams.entityId ?? '',
            runParams.parentPersonaId,
            undefined,
            runParams.sentinelName ?? pipeline.name,
          );
        }

        return transformPayload(params, {
          success: true,
          handle: result.handle,
          completed: false,
        });
      } else {
        // Synchronous: wait for pipeline completion, return results
        const result = await rustClient.sentinelExecute(sentinelRunParams);

        // Parse step results from output if available
        let stepResults: unknown[] | undefined;
        if (result.output) {
          try {
            const parsed = JSON.parse(result.output);
            if (Array.isArray(parsed)) {
              stepResults = parsed;
            } else if (parsed.stepResults) {
              stepResults = parsed.stepResults;
            }
          } catch {
            // Output wasn't JSON — that's fine, raw text is also valid
          }
        }

        return transformPayload(params, {
          success: result.success,
          handle: result.handle,
          completed: true,
          output: result.output,
          data: {
            success: result.success,
            stepResults,
            durationMs: undefined,
            error: result.success ? undefined : result.output,
          },
        });
      }
    } catch (error: any) {
      return transformPayload(params, {
        success: false,
        completed: true,
        error: error.message,
      });
    }
  }
}
