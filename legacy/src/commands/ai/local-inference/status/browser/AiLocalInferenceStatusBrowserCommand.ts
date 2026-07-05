/**
 * Ai Local Inference Status Command - Browser Implementation
 *
 * Query Continuum's local inference HTTP server (Anthropic-compatible Messages API). Returns whether the server is running and the URL external agents (Claude Code via ANTHROPIC_BASE_URL, future Codex via OPENAI_BASE_URL) should point at to use local Continuum models instead of cloud APIs. First-class surface for the AGENT-BACKBONE integration story (PR #976 §1-§4).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { AiLocalInferenceStatusParams, AiLocalInferenceStatusResult } from '../shared/AiLocalInferenceStatusTypes';

export class AiLocalInferenceStatusBrowserCommand extends CommandBase<AiLocalInferenceStatusParams, AiLocalInferenceStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/local-inference/status', context, subpath, commander);
  }

  async execute(params: AiLocalInferenceStatusParams): Promise<AiLocalInferenceStatusResult> {
    console.log('🌐 BROWSER: Delegating Ai Local Inference Status to server');
    return await this.remoteExecute(params);
  }
}
