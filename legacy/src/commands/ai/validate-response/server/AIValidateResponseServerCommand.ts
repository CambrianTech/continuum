/**
 * AI Validate-Response Server Command
 *
 * Thin TS shim — delegates to the Rust cognition/validate-response IPC.
 * Rust owns the prompt, model call, and one-word decision parser
 * (cognition/validate_response.rs). This command maps the public params
 * shape into the IPC request and forwards the typed decision back.
 *
 * Replaces the previous parallel reimplementation (which carried its
 * own prompt template + decision parser inline). Per Joel directive
 * 2026-05-18 19:44Z: zero-users full-blown-Rust-dev mode — single PR
 * adds the Rust path AND deletes the TS predecessor, no migration
 * cadence.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIValidateResponseParams, AIValidateResponseResult } from '../shared/AIValidateResponseTypes';
import { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class AIValidateResponseServerCommand extends CommandBase<AIValidateResponseParams, AIValidateResponseResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/validate-response', context, subpath, commander);
  }

  async execute(params: AIValidateResponseParams): Promise<AIValidateResponseResult> {
    try {
      const client = await RustCoreIPCClient.getInstanceAsync();
      const decision = await client.cognitionValidateResponseDecision({
        generatedResponse: params.generatedResponse,
        originalQuestion: params.originalQuestion,
        questionSender: params.questionSender,
        model: params.model,
      });

      return {
        context: params.context,
        sessionId: params.sessionId,
        decision: decision.decision,
        confidence: decision.confidence,
        reason: decision.reason,
        debug: params.verbose ? {
          promptSent: '(Rust-owned — see cognition::validate_response logs)',
          aiResponse: '(Rust-owned — see cognition::validate_response logs)',
        } : undefined,
      };
    } catch (error) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        error: error instanceof Error ? error.message : String(error),
        decision: 'SUBMIT',  // Fail-open: ship the draft when validator fails
        confidence: 0.0,
        reason: `Validation error: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }
}
