/**
 * Sentinel Run Command - Server Implementation
 *
 * Fire-and-forget wrapper that forwards to Rust SentinelModule.
 * Returns handle immediately. Status via sentinel/status.
 *
 * Escalation metadata (parentPersonaId, entityId, etc.) is passed to Rust,
 * which owns the lifecycle and pushes completion to sentinel/escalate.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelRunParams, SentinelRunResult } from '../shared/SentinelRunTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { Pipeline } from '../../../../workers/continuum-core/bindings/modules/sentinel';

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
    const asyncMode = (params as SentinelRunParams).async !== false;

    const timeoutSecs = (params as SentinelRunParams).timeout
      || definition.timeoutSecs
      || definition.timeout_secs;

    const pipeline: Pipeline = {
      name: definition.name || 'unnamed',
      steps: definition.steps,
      workingDir,
      timeoutSecs,
      inputs: definition.inputs || {},
    };

    const runParams = params as SentinelRunParams;
    const rustClient = RustCoreIPCClient.getInstance();

    // Build Rust params — escalation metadata travels with the sentinel
    const sentinelRunParams: Record<string, unknown> = {
      type: 'pipeline',
      command: 'pipeline',
      args: [] as string[],
      workingDir,
      env: { PIPELINE_JSON: JSON.stringify(pipeline) },
      timeout: timeoutSecs,
    };

    // Pass escalation metadata to Rust — it owns the lifecycle and will
    // push to sentinel/escalate on completion. No TS-side tracking needed.
    if (runParams.parentPersonaId) {
      sentinelRunParams.parentPersonaId = runParams.parentPersonaId;
    }
    if (runParams.entityId) {
      sentinelRunParams.entityId = runParams.entityId;
    }
    if (runParams.sentinelName || pipeline.name !== 'unnamed') {
      sentinelRunParams.sentinelName = runParams.sentinelName ?? pipeline.name;
    }

    try {
      if (asyncMode) {
        const result = await rustClient.sentinelRun(sentinelRunParams as any);

        return transformPayload(params, {
          success: true,
          handle: result.handle,
          completed: false,
        });
      } else {
        const result = await rustClient.sentinelExecute(sentinelRunParams as any);

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
            // Output wasn't JSON — raw text is valid
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
