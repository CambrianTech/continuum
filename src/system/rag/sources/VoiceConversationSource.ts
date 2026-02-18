/**
 * VoiceConversationSource - Loads voice transcription history for RAG context
 *
 * Unlike ConversationHistorySource (which loads persisted chat messages),
 * this source loads real-time voice transcriptions from VoiceOrchestrator's
 * session context.
 *
 * Key features:
 * - Speaker type labels: Each message prefixed with [HUMAN], [AI], or [AGENT]
 * - Real-time context: Loads from VoiceOrchestrator's recentUtterances
 * - Shorter history: Voice is real-time, so fewer messages needed
 * - Session-scoped: Only loads from the active voice session
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { LLMMessage } from '../shared/RAGTypes';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('VoiceConversationSource', 'rag');

// Token budget is lower for voice - real-time conversations are shorter
const TOKENS_PER_UTTERANCE_ESTIMATE = 30;  // Voice utterances are typically shorter

/**
 * Utterance event structure (from VoiceOrchestrator)
 */
interface UtteranceEvent {
  sessionId: string;
  speakerId: string;
  speakerName: string;
  speakerType: 'human' | 'persona' | 'agent';
  transcript: string;
  confidence: number;
  timestamp: number;
}

/**
 * VoiceOrchestrator interface for getting session context
 * Avoids circular imports by using interface
 */
interface VoiceOrchestratorInterface {
  getRecentUtterances(sessionId: string, limit?: number): UtteranceEvent[];
}

// Singleton reference to VoiceOrchestrator (set by VoiceOrchestrator on init)
let voiceOrchestrator: VoiceOrchestratorInterface | null = null;

/**
 * Register VoiceOrchestrator instance for RAG access
 * Called by VoiceOrchestrator on initialization
 */
export function registerVoiceOrchestrator(orchestrator: VoiceOrchestratorInterface): void {
  voiceOrchestrator = orchestrator;
  log.info('VoiceOrchestrator registered with VoiceConversationSource');
}

/**
 * Unregister VoiceOrchestrator (for cleanup)
 */
export function unregisterVoiceOrchestrator(): void {
  voiceOrchestrator = null;
}

export class VoiceConversationSource implements RAGSource {
  readonly name = 'voice-conversation';
  readonly priority = 85;  // High - voice context is critical for real-time response
  readonly defaultBudgetPercent = 30;  // Less than chat - voice is shorter

  /**
   * Only applicable when:
   * 1. We have a voice session ID in options
   * 2. VoiceOrchestrator is registered
   */
  isApplicable(context: RAGSourceContext): boolean {
    const hasVoiceSession = !!(context.options as any)?.voiceSessionId;
    const hasOrchestrator = voiceOrchestrator !== null;

    if (hasVoiceSession && !hasOrchestrator) {
      log.warn('Voice session requested but VoiceOrchestrator not registered');
    }

    return hasVoiceSession && hasOrchestrator;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    if (!voiceOrchestrator) {
      return this.emptySection(startTime, 'VoiceOrchestrator not registered');
    }

    const voiceSessionId = (context.options as any)?.voiceSessionId;
    if (!voiceSessionId) {
      return this.emptySection(startTime, 'No voice session ID');
    }

    // Calculate max utterances based on budget
    const maxUtterances = Math.max(5, Math.floor(allocatedBudget / TOKENS_PER_UTTERANCE_ESTIMATE));

    try {
      // Get recent utterances from VoiceOrchestrator
      const utterances = voiceOrchestrator.getRecentUtterances(voiceSessionId, maxUtterances);

      if (utterances.length === 0) {
        return this.emptySection(startTime);
      }

      // Convert to LLM message format with speaker type labels
      const llmMessages: LLMMessage[] = utterances.map((utterance) => {
        // Role assignment: own messages = 'assistant', others = 'user'
        const isOwnMessage = utterance.speakerId === context.personaId;
        const role = isOwnMessage ? 'assistant' as const : 'user' as const;

        // Format speaker type label
        const speakerTypeLabel = this.getSpeakerTypeLabel(utterance.speakerType);

        // Include speaker type in the message so AI clearly knows who's speaking
        // Format: "[HUMAN] Joel: Hello everyone"
        const formattedContent = `${speakerTypeLabel} ${utterance.speakerName}: ${utterance.transcript}`;

        return {
          role,
          content: formattedContent,
          name: utterance.speakerName,
          timestamp: utterance.timestamp
        };
      });

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = llmMessages.reduce((sum, m) => sum + this.estimateTokens(m.content), 0);

      log.debug(`Loaded ${llmMessages.length} voice utterances in ${loadTimeMs.toFixed(1)}ms (~${tokenCount} tokens)`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        messages: llmMessages,
        systemPromptSection: this.buildVoiceSystemPromptSection(utterances),
        metadata: {
          utteranceCount: llmMessages.length,
          voiceSessionId,
          personaId: context.personaId,
          speakerBreakdown: this.getSpeakerBreakdown(utterances),
          // Voice response style configuration - used by PersonaResponseGenerator
          // NOTE: Don't limit tokens artificially - that causes robotic mid-sentence cutoffs
          // Natural turn-taking handled by arbiter coordination, not hard limits
          responseStyle: {
            voiceMode: true,
            conversational: true,
            preferQuestions: true,   // Ask clarifying questions vs long explanations
            avoidFormatting: true    // No bullet points, code blocks, markdown
          }
        }
      };
    } catch (error: any) {
      log.error(`Failed to load voice conversation: ${error.message}`);
      return this.emptySection(startTime, error.message);
    }
  }

  /**
   * Build voice-specific system prompt section
   * Explains the speaker type labels and CRITICAL brevity requirements
   */
  private buildVoiceSystemPromptSection(utterances: UtteranceEvent[]): string {
    const humanCount = utterances.filter(u => u.speakerType === 'human').length;
    const aiCount = utterances.filter(u => u.speakerType === 'persona' || u.speakerType === 'agent').length;

    return `## 🎙️ VOICE CALL CONTEXT

You are in a LIVE VOICE CONVERSATION. Your response will be spoken aloud via TTS.

**Speaker Labels:**
- [HUMAN] - Human participants
- [AI] - AI participants (other personas)
- [AGENT] - AI agents (like Claude Code)

**Session:** ${humanCount} human + ${aiCount} AI utterances

**VOICE CONVERSATION STYLE:**

1. **NO FORMATTING** - No bullets, lists, code blocks, or markdown
2. **SPEAK NATURALLY** - As if talking face-to-face in a real conversation
3. **CONVERSATIONAL FLOW** - Complete your thoughts naturally, don't cut off mid-sentence
4. **BE RESPONSIVE** - Listen to what others are saying, engage with their points
5. **NATURAL PACING** - Speak at a comfortable length, neither too brief nor too long

You may speak for as long as needed to complete your thought. Natural conversation has varying lengths.`;
  }

  /**
   * Get speaker type label for message formatting
   */
  private getSpeakerTypeLabel(speakerType: 'human' | 'persona' | 'agent'): string {
    switch (speakerType) {
      case 'human':
        return '[HUMAN]';
      case 'persona':
        return '[AI]';
      case 'agent':
        return '[AGENT]';
      default:
        return '[UNKNOWN]';
    }
  }

  /**
   * Get breakdown of speakers by type
   */
  private getSpeakerBreakdown(utterances: UtteranceEvent[]): Record<string, number> {
    const breakdown: Record<string, number> = {
      human: 0,
      persona: 0,
      agent: 0
    };

    for (const utterance of utterances) {
      breakdown[utterance.speakerType] = (breakdown[utterance.speakerType] || 0) + 1;
    }

    return breakdown;
  }

  private emptySection(startTime: number, error?: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      messages: [],
      metadata: error ? { error } : {}
    };
  }

  private estimateTokens(text: string): number {
    // Rough estimate: ~4 characters per token
    return Math.ceil(text.length / 4);
  }
}
