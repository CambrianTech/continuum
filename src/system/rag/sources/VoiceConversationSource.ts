/**
 * VoiceConversationSource - Loads voice conversation timeline for RAG context
 *
 * Builds LLM context from VoiceSessionTimeline — the ordered, sequenced,
 * cursor-tracked conversation log that ALL participants (human + AI) share.
 *
 * Like chat history, every member sees the full chain including themselves.
 * Each persona has a cursor tracking where they are in the timeline, so they get:
 * - Backfill: recent conversation for context (what happened before)
 * - New: what happened since they last spoke/processed
 *
 * Fragment consolidation and "..." continuity markers are handled at ingestion
 * by VoiceSessionTimeline — this source just reads the clean, ordered chain.
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { LLMMessage } from '../shared/RAGTypes';
import { extractSentiment, formatEmotionLabel } from '../shared/TextSentiment';
import { Logger } from '../../core/logging/Logger';
import type { VoiceSessionTimeline, TimelineEntry, TimelineSlice } from '../../voice/server/VoiceSessionTimeline';
import type { UUID } from '../../core/types/CrossPlatformUUID';

const log = Logger.create('VoiceConversationSource', 'rag');

const TOKENS_PER_TURN_ESTIMATE = 30;

/**
 * VoiceOrchestrator interface — avoids circular imports.
 * Exposes both legacy (getRecentUtterances) and cursor-aware (getTimeline) paths.
 */
interface VoiceOrchestratorInterface {
  getTimeline(sessionId: string): VoiceSessionTimeline | null;
}

// Singleton reference (set by VoiceOrchestrator on init)
let voiceOrchestrator: VoiceOrchestratorInterface | null = null;

/**
 * Register VoiceOrchestrator instance for RAG access.
 * Called by VoiceOrchestrator on initialization.
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
  readonly priority = 85;
  readonly defaultBudgetPercent = 30;

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

    const timeline = voiceOrchestrator.getTimeline(voiceSessionId);
    if (!timeline) {
      return this.emptySection(startTime, 'No timeline for session');
    }

    const maxTurns = Math.max(5, Math.floor(allocatedBudget / TOKENS_PER_TURN_ESTIMATE));
    const personaId = context.personaId as UUID;

    try {
      // Get cursor-aware slice: backfill (context) + new (unseen) entries.
      // Every participant sees the full ordered chain INCLUDING their own responses.
      // This is exactly like chat history — you must follow the conversation thread.
      const slice = timeline.sliceFor(personaId, maxTurns, maxTurns);

      // Merge backfill + new into one ordered conversation chain
      const allEntries = [...slice.backfill, ...slice.newEntries];

      if (allEntries.length === 0) {
        return this.emptySection(startTime);
      }

      // Convert to LLM messages — same speaker labeling as before
      const llmMessages: LLMMessage[] = allEntries.map((entry) => {
        const isOwnMessage = entry.speakerId === personaId;
        const role = isOwnMessage ? 'assistant' as const : 'user' as const;

        const speakerTypeLabel = this.getSpeakerTypeLabel(entry.speakerType);
        const sentiment = extractSentiment(entry.transcript);
        const emotionLabel = formatEmotionLabel(sentiment);

        const nameWithEmotion = emotionLabel
          ? `${entry.speakerName} (${emotionLabel})`
          : entry.speakerName;
        const formattedContent = `${speakerTypeLabel} ${nameWithEmotion}: ${entry.transcript}`;

        return {
          role,
          content: formattedContent,
          name: entry.speakerName,
          timestamp: entry.timestamp,
        };
      });

      // Advance cursor — this persona has now seen up to headSeq.
      // Next inference will only get entries after this point as "new".
      timeline.advanceCursor(personaId, slice.headSeq);

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = llmMessages.reduce((sum, m) => sum + this.estimateTokens(m.content), 0);

      log.debug(
        `Voice RAG for ${personaId}: ${slice.backfill.length} backfill + ${slice.newEntries.length} new = ${allEntries.length} turns (~${tokenCount} tokens, ${loadTimeMs.toFixed(1)}ms)`
      );

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        messages: llmMessages,
        systemPromptSection: this.buildVoiceSystemPromptSection(allEntries, slice),
        metadata: {
          turns: allEntries.length,
          backfillTurns: slice.backfill.length,
          newTurns: slice.newEntries.length,
          headSeq: slice.headSeq,
          totalSessionTurns: slice.totalTurns,
          voiceSessionId,
          personaId,
          speakerBreakdown: this.getSpeakerBreakdown(allEntries),
          responseStyle: {
            voiceMode: true,
            conversational: true,
            preferQuestions: true,
            avoidFormatting: true,
          },
        },
      };
    } catch (error: any) {
      log.error(`Failed to load voice conversation: ${error.message}`);
      return this.emptySection(startTime, error.message);
    }
  }

  /**
   * Build voice-specific system prompt section.
   * Includes the new/backfill context so the persona understands
   * what's fresh vs what it already responded to.
   */
  private buildVoiceSystemPromptSection(entries: TimelineEntry[], slice: TimelineSlice): string {
    const humanCount = entries.filter(e => e.speakerType === 'human').length;
    const aiCount = entries.filter(e => e.speakerType === 'persona' || e.speakerType === 'agent').length;
    const emotionalContext = this.buildEmotionalContext(entries);

    // Tell the persona how much is new vs context
    const newCount = slice.newEntries.length;
    const contextNote = newCount > 0
      ? `**New since you last spoke:** ${newCount} turn${newCount > 1 ? 's' : ''} (respond to these)\n`
      : '';

    return `## VOICE CALL CONTEXT

You are in a LIVE VOICE CONVERSATION. Your response will be spoken aloud via TTS.
Each participant has an avatar whose facial expressions and body gestures reflect their emotional state.

**Speaker Labels:**
- [HUMAN] - Human participants
- [AI] - AI participants (other personas)
- [AGENT] - AI agents (like Claude Code)

**Speech Continuity ("..."):**
When you see "..." within a message, it means the speaker's voice was captured across multiple segments.
For example: "I'm thinking about whether... we should refactor... the auth module"
This is ONE continuous thought, not hesitation. The speaker is still talking — wait for a complete turn before responding.

**Emotional Annotations:**
Messages include emotional state when detected, shown as: Speaker (emotion, gesture): text
These reflect the speaker's avatar expression and body language. React naturally to others' emotions.

**Session:** ${humanCount} human + ${aiCount} AI turns in your view
${contextNote}${emotionalContext}
**VOICE CONVERSATION STYLE:**

1. **NO FORMATTING** - No bullets, lists, code blocks, or markdown
2. **SPEAK NATURALLY** - As if talking face-to-face in a real conversation
3. **FOLLOW THE THREAD** - You can see the full conversation chain including your own previous responses. Build on what was said. Don't repeat yourself or others.
4. **BE RESPONSIVE** - Engage with the most recent points. If multiple people spoke since you last did, acknowledge the thread.
5. **BE EXPRESSIVE** - Let your personality come through. Say "wow" when surprised, "hmm" when thinking, "absolutely" when you agree.
6. **READ THE ROOM** - Notice others' emotional state from the annotations. Mirror and respond to the emotional tone.

You may speak for as long as needed to complete your thought. Natural conversation has varying lengths.`;
  }

  private buildEmotionalContext(entries: TimelineEntry[]): string {
    const emotionCounts: Record<string, number> = {};

    for (const e of entries) {
      const s = extractSentiment(e.transcript);
      if (s.emotion !== 'neutral') {
        emotionCounts[s.emotion] = (emotionCounts[s.emotion] || 0) + 1;
      }
    }

    const emotionEntries = Object.entries(emotionCounts).sort((a, b) => b[1] - a[1]);
    if (emotionEntries.length === 0) return '';

    const dominant = emotionEntries[0][0];
    const toneDescriptions: Record<string, string> = {
      happy: 'warm and positive',
      sad: 'somber and reflective',
      angry: 'tense and frustrated',
      surprised: 'energetic and reactive',
      relaxed: 'calm and easy-going',
    };

    const tone = toneDescriptions[dominant] || dominant;
    return `**Conversation mood:** ${tone}\n`;
  }

  private getSpeakerTypeLabel(speakerType: 'human' | 'persona' | 'agent'): string {
    switch (speakerType) {
      case 'human': return '[HUMAN]';
      case 'persona': return '[AI]';
      case 'agent': return '[AGENT]';
      default: return '[UNKNOWN]';
    }
  }

  private getSpeakerBreakdown(entries: TimelineEntry[]): Record<string, number> {
    const breakdown: Record<string, number> = { human: 0, persona: 0, agent: 0 };
    for (const entry of entries) {
      breakdown[entry.speakerType] = (breakdown[entry.speakerType] || 0) + 1;
    }
    return breakdown;
  }

  private emptySection(startTime: number, error?: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      messages: [],
      metadata: error ? { error } : {},
    };
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
