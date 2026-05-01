/**
 * Ai Local Inference Start Command - Browser Implementation
 *
 * Ensure Continuum's local inference HTTP server is running and return its URL. Idempotent — if already running, returns the existing URL without restarting. External agents (Claude Code via ANTHROPIC_BASE_URL, future Codex via OPENAI_BASE_URL) should call this once at startup, then use the returned URL. First-class surface for the AGENT-BACKBONE integration story (PR #976 §1-§4); previously only reachable as the Sentinel-internal sentinel/local-inference-start IPC command.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiLocalInferenceStartParams, AiLocalInferenceStartResult } from '../shared/AiLocalInferenceStartTypes';

export class AiLocalInferenceStartBrowserCommand extends CommandBase<AiLocalInferenceStartParams, AiLocalInferenceStartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/local-inference/start', context, subpath, commander);
  }

  async execute(params: AiLocalInferenceStartParams): Promise<AiLocalInferenceStartResult> {
    console.log('🌐 BROWSER: Delegating Ai Local Inference Start to server');
    return await this.remoteExecute(params);
  }
}
