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

    // Build pipeline for Rust
    const pipeline: Pipeline = {
      name: definition.name || 'unnamed',
      steps: definition.steps,
      workingDir,
      timeoutSecs: definition.timeoutSecs || definition.timeout_secs,
      inputs: definition.inputs || {},
    };

    // Route to Rust sentinel/execute (NOT sentinel/pipeline)
    // sentinel/execute spawns a task and returns handle immediately
    const rustClient = RustCoreIPCClient.getInstance();

    try {
      // Use sentinel/run which spawns a task for the pipeline
      const result = await rustClient.sentinelRun({
        type: 'pipeline',
        command: 'pipeline',  // Internal: tells Rust this is a pipeline
        args: [],
        workingDir,
        env: { PIPELINE_JSON: JSON.stringify(pipeline) },
      });

      return transformPayload(params, {
        success: true,
        handle: result.handle,
        completed: false,  // Not completed - running in background
      });
    } catch (error: any) {
      return transformPayload(params, {
        success: false,
        completed: true,
        error: error.message,
      });
    }
  }
}
