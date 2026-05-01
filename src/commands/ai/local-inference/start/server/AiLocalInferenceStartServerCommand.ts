/**
 * Ai Local Inference Start Command - Server Implementation
 *
 * Ensure Continuum's local inference HTTP server is running and return
 * its URL. Idempotent — if already running, returns the existing URL
 * without restarting. First-class surface for AGENT-BACKBONE-INTEGRATION
 * (PR #976 §1-§4); previously only reachable as the Sentinel-internal
 * `sentinel/local-inference-start` IPC command.
 *
 * External-agent setup pattern:
 *   const { url } = await Commands.execute('ai/local-inference/start');
 *   process.env.ANTHROPIC_BASE_URL = url;   // for Claude Code SDK
 *   // OR (when openai_compat.rs lands per AGENT-BACKBONE §4.1):
 *   process.env.OPENAI_BASE_URL = `${url}`; // for Codex / openclaws
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiLocalInferenceStartParams, AiLocalInferenceStartResult } from '../shared/AiLocalInferenceStartTypes';
import { createAiLocalInferenceStartResultFromParams } from '../shared/AiLocalInferenceStartTypes';
import { RustCoreIPCClient } from '../../../../../workers/continuum-core/bindings/RustCoreIPC';

export class AiLocalInferenceStartServerCommand extends CommandBase<AiLocalInferenceStartParams, AiLocalInferenceStartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/local-inference/start', context, subpath, commander);
  }

  async execute(params: AiLocalInferenceStartParams): Promise<AiLocalInferenceStartResult> {
    const ipc = await RustCoreIPCClient.getInstanceAsync();

    // Probe first so we can report alreadyRunning accurately. The Rust
    // start path is idempotent (OnceCell-guarded in http/mod.rs), so this
    // probe + start sequence has no race risk — at worst we report
    // alreadyRunning=false on a millisecond-tight race, which is
    // diagnostic noise, not a correctness issue.
    const probe = await ipc.sentinelLocalInferencePort();
    const wasRunning = !!(probe.success && probe.port && probe.url);

    const result = await ipc.sentinelLocalInferenceStart();

    if (!result.success || !result.url || !result.port) {
      throw new Error(
        `Failed to start local inference HTTP server: ${result.error ?? 'unknown'}. ` +
        `Check that continuum-core-server is running (continuum#722 covers the supervised lifecycle).`
      );
    }

    return createAiLocalInferenceStartResultFromParams(params, {
      success: true,
      url: result.url,
      port: result.port,
      protocol: 'anthropic',
      alreadyRunning: wasRunning,
    });
  }
}
