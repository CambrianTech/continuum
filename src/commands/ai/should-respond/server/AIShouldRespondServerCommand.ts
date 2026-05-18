/**
 * AI Should-Respond Server Command
 *
 * Thin TS shim — delegates to the Rust cognition/should-respond IPC
 * (cognition/should_respond.rs). Rust owns the gating prompt, model
 * call, and parser; this command maps the public params shape into
 * the IPC request and forwards the typed decision back.
 *
 * Prior to continuum#1420 this command carried a parallel
 * reimplementation of gating with a stale prompt + JSON-repair retry
 * loop — that drifted from the canonical Rust path used by
 * AIDecisionService.evaluateGating. The delegation removes both
 * paths' divergence risk.
 */

import { AIShouldRespondCommand } from '../shared/AIShouldRespondCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AIShouldRespondParams, AIShouldRespondResult } from '../shared/AIShouldRespondTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import type {
  AIDecisionContext as RustAIDecisionContext,
} from '../../../../shared/generated';

export class AIShouldRespondServerCommand extends AIShouldRespondCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/should-respond', context, subpath, commander);
  }

  async execute(params: AIShouldRespondParams): Promise<AIShouldRespondResult> {
    try {
      if (!params.ragContext) {
        throw new Error('ragContext is required for LLM strategy');
      }

      // Build the Rust IPC context from the public params shape.
      // The Rust side (cognition/should_respond.rs::AIDecisionContext)
      // structurally matches the TS RAGContext fields we forward;
      // the cast mirrors what AIDecisionService.evaluateGating does
      // for the same surface.
      const context = {
        personaId: params.personaId,
        personaName: params.personaName,
        roomId: params.contextId,
        triggerMessage: {
          // Rust requires a stable id on the trigger. Params don't
          // carry one (callers identify the message by content +
          // sender timestamp); synthesize a deterministic-looking
          // id from the timestamp so repeat calls don't multiply
          // observability noise.
          id: `trigger-${params.triggerMessage.timestamp}`,
          senderName: params.triggerMessage.senderName,
          content: { text: params.triggerMessage.content },
        },
        ragContext: params.ragContext,
        systemPrompt: params.ragContext.identity?.systemPrompt,
      } as unknown as RustAIDecisionContext;

      const client = await RustCoreIPCClient.getInstanceAsync();
      const decision = await client.cognitionShouldRespond({
        context,
        model: params.model,
      });

      // Verbose debug surface: TS keeps message count + preview
      // (derivable from params without Rust round-trip). Dropped:
      // `promptSent` + `aiResponse` (Rust owns prompt assembly +
      // sees the raw response; operator inspects Rust logs at
      // `cognition::should_respond` for that detail).
      let debugOutput: AIShouldRespondResult['debug'] = undefined;
      if (params.verbose) {
        const conversationText = params.ragContext.conversationHistory
          .map(msg => `${msg.role}: ${msg.content}`)
          .join('\n');
        debugOutput = {
          ragContext: {
            messageCount: params.ragContext.conversationHistory.length,
            conversationPreview:
              conversationText.substring(0, 500) +
              (conversationText.length > 500 ? '...' : ''),
          },
          promptSent: '(Rust-owned — see cognition::should_respond logs)',
          aiResponse: '(Rust-owned — see cognition::should_respond logs)',
        };
      }

      return {
        context: params.context,
        sessionId: params.sessionId,
        shouldRespond: decision.shouldRespond,
        confidence: decision.confidence,
        reason: decision.reason,
        factors: decision.factors ?? {
          mentioned: false,
          questionAsked: false,
          domainRelevant: false,
          recentlySpoke: false,
          othersAnswered: false,
        },
        debug: debugOutput,
      };
    } catch (error) {
      console.error('❌ AI Should-Respond: Command failed:', error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        error: error instanceof Error ? error.message : String(error),
        shouldRespond: false,
        confidence: 0.0,
        reason: 'Command execution failed',
        factors: {
          mentioned: false,
          questionAsked: false,
          domainRelevant: false,
          recentlySpoke: false,
          othersAnswered: false,
        },
      };
    }
  }
}
