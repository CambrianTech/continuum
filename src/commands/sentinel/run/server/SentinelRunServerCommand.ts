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
import type { PipelineSentinelParams, SentinelRunParams, SentinelRunResult } from '../shared/SentinelRunTypes';
import type { PipelineSentinelDefinition } from '../../../../system/sentinel/SentinelDefinition';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type { Pipeline, PipelineStep, SentinelRunParams as SentinelIPCParams } from '../../../../workers/continuum-core/bindings/modules/sentinel';
import { TemplateRegistry } from '../../../../system/sentinel/pipelines/TemplateRegistry';
import { sentinelEventBridge } from '../../../../system/sentinel/SentinelEventBridge';
import { announceSentinelStart } from '../../../../system/sentinel/SentinelChatBridge';

export class SentinelRunServerCommand extends CommandBase<SentinelRunParams, SentinelRunResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/run', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelRunResult> {
    // Cast once at wire boundary — params arrives as JTAGPayload from CLI/IPC
    const runParams = params as JTAGPayload & PipelineSentinelParams & { steps?: PipelineSentinelDefinition['steps'] };

    // Parse definition — raw JSON may include Rust-side fields (timeoutSecs, inputs)
    type PipelineDefinitionJSON = PipelineSentinelDefinition & {
      timeoutSecs?: number;
      timeout_secs?: number;
      inputs?: Record<string, unknown>;
    };
    let definition: PipelineDefinitionJSON | undefined;

    // Template resolution — build pipeline from named template + config
    if (runParams.template) {
      const templateName = runParams.template;
      if (!TemplateRegistry.has(templateName)) {
        const available = TemplateRegistry.list().map(t => `${t.name} — ${t.description}`).join('\n  ');
        return transformPayload(params, {
          success: false,
          completed: true,
          error: `Template '${templateName}' not found.\n\nAvailable templates:\n  ${available}`,
        });
      }

      const templateConfig = {
        ...runParams.templateConfig,
        // Common fields auto-populated from sentinel params
        ...(runParams.parentPersonaId && { personaId: runParams.parentPersonaId }),
        ...(runParams.workingDir && { cwd: runParams.workingDir }),
      };

      const pipeline = TemplateRegistry.build(templateName, templateConfig);
      // Convert Pipeline to PipelineDefinitionJSON format
      // version/loop are required by PipelineSentinelDefinition but not used by Rust Pipeline
      definition = {
        type: 'pipeline',
        name: pipeline.name || templateName,
        version: '1.0',
        loop: { type: 'once' },
        steps: pipeline.steps as PipelineDefinitionJSON['steps'],
        timeoutSecs: pipeline.timeoutSecs,
        inputs: pipeline.inputs as Record<string, unknown>,
      };
    } else if (runParams.definition) {
      definition = typeof runParams.definition === 'string'
        ? JSON.parse(runParams.definition) as PipelineDefinitionJSON
        : runParams.definition as PipelineDefinitionJSON;
    } else if (runParams.steps) {
      definition = { type: 'pipeline', name: 'unnamed', steps: runParams.steps } as PipelineDefinitionJSON;
    }

    if (!definition) {
      const available = TemplateRegistry.list().map(t => `${t.name} — ${t.description}`).join('\n  ');
      return transformPayload(params, {
        success: false,
        completed: true,
        error: `Missing pipeline definition. Provide --definition, --steps, or --template.\n\nAvailable templates:\n  ${available}`,
      });
    }

    const workingDir = runParams.workingDir || process.cwd();
    const asyncMode = runParams.async !== false;

    const timeoutSecs = runParams.timeout
      || definition.timeoutSecs
      || definition.timeout_secs;

    const pipeline: Pipeline = {
      name: definition.name || 'unnamed',
      steps: definition.steps as PipelineStep[],
      workingDir,
      timeoutSecs,
      inputs: definition.inputs || {},
    };

    const rustClient = RustCoreIPCClient.getInstance();

    // Build Rust IPC params — escalation metadata travels with the sentinel
    const ipcParams: SentinelIPCParams = {
      type: 'pipeline',
      command: 'pipeline',
      args: [],
      workingDir,
      env: { PIPELINE_JSON: JSON.stringify(pipeline) },
      timeout: timeoutSecs,
      parentPersonaId: runParams.parentPersonaId,
      entityId: runParams.entityId,
      sentinelName: runParams.sentinelName ?? (pipeline.name !== 'unnamed' ? pipeline.name : undefined),
    };

    try {
      if (asyncMode) {
        const result = await rustClient.sentinelRun(ipcParams);

        // Wire up event bridge so progress/completion events flow to TypeScript
        const roomId = (runParams.templateConfig as Record<string, unknown> | undefined)?.roomId as string | undefined;
        sentinelEventBridge.watch(result.handle, 'pipeline', {
          personaId: runParams.parentPersonaId,
          entityId: runParams.entityId,
          sentinelName: ipcParams.sentinelName,
          template: runParams.template,
          roomId,
        });

        // Announce to chat room if one is associated
        if (roomId) {
          const templateConfig = runParams.templateConfig as Record<string, unknown> | undefined;
          announceSentinelStart(
            roomId,
            ipcParams.sentinelName ?? runParams.template ?? 'pipeline',
            result.handle,
            templateConfig?.personaName as string | undefined,
            runParams.parentPersonaId,
          );
        }

        return transformPayload(params, {
          success: true,
          handle: result.handle,
          completed: false,
        });
      } else {
        const result = await rustClient.sentinelExecute(ipcParams);

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
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      return transformPayload(params, {
        success: false,
        completed: true,
        error: message,
      });
    }
  }
}
