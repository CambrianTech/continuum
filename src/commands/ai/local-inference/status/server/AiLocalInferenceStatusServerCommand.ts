/**
 * Ai Local Inference Status Command - Server Implementation
 *
 * Query Continuum's local inference HTTP server (Anthropic-compatible
 * Messages API). First-class surface for AGENT-BACKBONE-INTEGRATION
 * (PR #976 §1-§4) — wraps the existing Sentinel-internal IPC command
 * `sentinel/local-inference-port` so any caller (Codex hook setup,
 * openclaws integration, future external-agent shims, the docs) can
 * discover the local URL without reaching into Sentinel internals.
 *
 * Returns running=false (with empty url + port=0) when the server has
 * never been started — call `ai/local-inference/start` to bring it up
 * (idempotent).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiLocalInferenceStatusParams, AiLocalInferenceStatusResult } from '../shared/AiLocalInferenceStatusTypes';
import { createAiLocalInferenceStatusResultFromParams } from '../shared/AiLocalInferenceStatusTypes';
import { RustCoreIPCClient } from '../../../../../workers/continuum-core/bindings/RustCoreIPC';

export class AiLocalInferenceStatusServerCommand extends CommandBase<AiLocalInferenceStatusParams, AiLocalInferenceStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/local-inference/status', context, subpath, commander);
  }

  async execute(params: AiLocalInferenceStatusParams): Promise<AiLocalInferenceStatusResult> {
    const ipc = await RustCoreIPCClient.getInstanceAsync();
    const probe = await ipc.sentinelLocalInferencePort();

    // sentinelLocalInferencePort returns { success: boolean, port?, url?, error? }
    // We translate to the cleaner first-class shape: running boolean + the
    // url/port iff actually serving. Empty url + port 0 when not running
    // — keeps consumers from accidentally pointing at a dead URL.
    const running = !!(probe.success && probe.port && probe.url);

    return createAiLocalInferenceStatusResultFromParams(params, {
      success: true,
      running,
      url: running ? (probe.url || '') : '',
      port: running ? (probe.port || 0) : 0,
      // Only Anthropic-compat is shipped today (workers/continuum-core/src/http/anthropic_compat.rs).
      // Will be 'openai' OR a comma-separated list once openai_compat.rs lands per AGENT-BACKBONE §4.1.
      protocol: 'anthropic',
    });
  }
}
