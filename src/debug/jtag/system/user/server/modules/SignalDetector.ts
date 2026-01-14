/**
 * SignalDetector - Detect training signals from user interactions using AI classification
 *
 * Part of the continuous micro-LoRA system. Uses semantic AI classification instead of
 * brittle regex patterns. Detects:
 * - Corrections: User is correcting AI's factual or logical errors
 * - Approvals: User is happy with the response quality
 * - Frustration: User is repeating themselves or expressing dissatisfaction
 * - Explicit feedback: User is giving direct instructions about behavior
 *
 * Each signal is classified by:
 * - Type: What kind of feedback
 * - Trait: Which trait adapter it applies to (tone, reasoning, expertise, etc.)
 * - Polarity: Positive (reinforce) or negative (correct)
 */

import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { TraitType } from '../../../genome/entities/GenomeLayerEntity';
import type { AIGenerateParams, AIGenerateResult } from '../../../../commands/ai/generate/shared/AIGenerateTypes';
import { Commands } from '../../../core/shared/Commands';
import { contentPreview } from '../../../../shared/utils/StringUtils';

/**
 * Signal types that can trigger training
 */
export type SignalType = 'correction' | 'approval' | 'frustration' | 'explicit_feedback' | 'none';

/**
 * Training polarity - should we reinforce or correct?
 */
export type SignalPolarity = 'positive' | 'negative';

/**
 * A detected training signal
 */
export interface TrainingSignal {
  type: SignalType;
  trait: TraitType;
  polarity: SignalPolarity;
  confidence: number;  // 0-1, how confident we are this is a real signal
  originalMessage: ChatMessageEntity | null;  // The AI message being corrected/approved
  userResponse: ChatMessageEntity;  // The user's feedback
  context: string;  // Formatted conversation context for training
  detectedAt: number;  // Timestamp
}

/**
 * AI classification result
 */
interface SignalClassification {
  isSignal: boolean;
  signalType: SignalType;
  trait: TraitType;
  polarity: SignalPolarity;
  confidence: number;
  reasoning: string;
}

/**
 * Common trait types
 */
export const TRAIT_TYPES = {
  TONE_AND_VOICE: 'tone_and_voice',
  REASONING_STYLE: 'reasoning_style',
  DOMAIN_EXPERTISE: 'domain_expertise',
  SOCIAL_DYNAMICS: 'social_dynamics',
  CREATIVE_EXPRESSION: 'creative_expression',
} as const;

/**
 * SignalDetector - Uses AI to classify user feedback semantically
 */
export class SignalDetector {
  private classificationCache: Map<string, SignalClassification> = new Map();
  private readonly CACHE_TTL_MS = 60000; // 1 minute cache

  /**
   * Detect a training signal from a user message using AI classification
   */
  async detectSignalAsync(
    message: ChatMessageEntity,
    precedingAIMessage: ChatMessageEntity | null,
    conversationHistory: ChatMessageEntity[]
  ): Promise<TrainingSignal | null> {
    // Content-based classification - any message can be feedback
    const text = message.content?.text || '';
    if (text.length < 3) {
      return null;  // Too short to be meaningful feedback
    }

    // Use AI to classify the signal
    const classification = await this.classifyWithAI(text, precedingAIMessage);

    if (!classification.isSignal || classification.confidence < 0.6) {
      return null;
    }

    // Build training context
    const context = this.buildContext(message, precedingAIMessage, conversationHistory);

    return {
      type: classification.signalType,
      trait: classification.trait,
      polarity: classification.polarity,
      confidence: classification.confidence,
      originalMessage: precedingAIMessage,
      userResponse: message,
      context,
      detectedAt: Date.now(),
    };
  }

  /**
   * Synchronous fallback using simple heuristics (for non-blocking path)
   * Only catches obvious signals - AI classification handles nuanced cases
   */
  detectSignal(
    message: ChatMessageEntity,
    precedingAIMessage: ChatMessageEntity | null,
    conversationHistory: ChatMessageEntity[]
  ): TrainingSignal | null {
    // Content-based classification - no sender type filtering
    const text = (message.content?.text || '').trim();
    if (text.length < 3) return null;

    // Quick heuristic check - only very obvious signals
    const classification = this.quickClassify(text);
    if (!classification.isSignal) return null;

    const context = this.buildContext(message, precedingAIMessage, conversationHistory);

    return {
      type: classification.signalType,
      trait: classification.trait,
      polarity: classification.polarity,
      confidence: classification.confidence,
      originalMessage: precedingAIMessage,
      userResponse: message,
      context,
      detectedAt: Date.now(),
    };
  }

  /**
   * Quick heuristic classification for obvious signals only
   * Defers to AI for anything ambiguous
   */
  private quickClassify(text: string): SignalClassification {
    const lower = text.toLowerCase();
    const noSignal: SignalClassification = {
      isSignal: false,
      signalType: 'none',
      trait: TRAIT_TYPES.TONE_AND_VOICE,
      polarity: 'negative',
      confidence: 0,
      reasoning: 'No obvious signal detected'
    };

    // Very short positive responses (high confidence approval)
    if (/^(perfect|exactly|thanks|great|yes)[!.]?$/i.test(text)) {
      return {
        isSignal: true,
        signalType: 'approval',
        trait: TRAIT_TYPES.TONE_AND_VOICE,
        polarity: 'positive',
        confidence: 0.9,
        reasoning: 'Short affirmative response'
      };
    }

    // Explicit correction starters
    if (/^(no,?\s|wrong|incorrect|that'?s\s+not)/i.test(text)) {
      return {
        isSignal: true,
        signalType: 'correction',
        trait: this.inferTraitFromContent(text),
        polarity: 'negative',
        confidence: 0.85,
        reasoning: 'Explicit correction indicator'
      };
    }

    // Explicit feedback about style/format
    if (/\b(too\s+(long|short|verbose|brief)|be\s+more\s+(concise|detailed))\b/i.test(text)) {
      return {
        isSignal: true,
        signalType: 'explicit_feedback',
        trait: TRAIT_TYPES.TONE_AND_VOICE,
        polarity: 'negative',
        confidence: 0.85,
        reasoning: 'Explicit style feedback'
      };
    }

    // Frustration indicators
    if (/\b(i\s+already|how\s+many\s+times)\b/i.test(text) || /\bagain:/i.test(text)) {
      return {
        isSignal: true,
        signalType: 'frustration',
        trait: TRAIT_TYPES.SOCIAL_DYNAMICS,
        polarity: 'negative',
        confidence: 0.8,
        reasoning: 'Frustration indicator'
      };
    }

    return noSignal;
  }

  /**
   * Use AI to classify signal type and trait semantically
   */
  private async classifyWithAI(
    userText: string,
    aiMessage: ChatMessageEntity | null
  ): Promise<SignalClassification> {
    // Check cache first
    const cacheKey = `${userText}|${aiMessage?.id || 'null'}`;
    const cached = this.classificationCache.get(cacheKey);
    if (cached) return cached;

    const prompt = this.buildClassificationPrompt(userText, aiMessage);

    try {
      const params: Partial<AIGenerateParams> = {
        messages: [{ role: 'user', content: prompt }],
        model: 'llama-3.1-8b-instant',  // Fast cloud model - don't block local inference queue
        preferredProvider: 'groq',       // Cloud API - fast (<1s) vs local (~10s)
        temperature: 0.1,                // Low temperature for consistent classification
        maxTokens: 200,
        systemPrompt: 'You are a signal classifier. Output ONLY valid JSON, no other text.'
      };

      const result = await Commands.execute('ai/generate', params) as AIGenerateResult;

      if (!result.success || !result.text) {
        return this.quickClassify(userText);  // Fallback to heuristics
      }

      const classification = this.parseClassificationResponse(result.text);

      // Cache result
      this.classificationCache.set(cacheKey, classification);
      setTimeout(() => this.classificationCache.delete(cacheKey), this.CACHE_TTL_MS);

      return classification;
    } catch (error) {
      console.error('[SignalDetector] AI classification failed:', error);
      return this.quickClassify(userText);  // Fallback to heuristics
    }
  }

  /**
   * Build prompt for AI classification
   */
  private buildClassificationPrompt(userText: string, aiMessage: ChatMessageEntity | null): string {
    const aiContext = aiMessage?.content
      ? `\nAI's previous response: "${contentPreview(aiMessage.content, 200)}..."`
      : '';

    return `Classify this user message as training feedback for an AI assistant.

User message: "${userText}"${aiContext}

Classify into ONE of:
- correction: User is correcting a factual/logical error
- approval: User is expressing satisfaction
- frustration: User is frustrated (repeating, exasperated)
- explicit_feedback: User is giving direct behavioral instructions
- none: Not feedback, just a normal message

Also classify the trait being addressed:
- tone_and_voice: Style, length, formality
- reasoning_style: Logic, explanation, problem-solving
- domain_expertise: Facts, accuracy, knowledge
- social_dynamics: Interaction style, empathy
- creative_expression: Creativity, originality

Output JSON only:
{"isSignal": boolean, "signalType": "...", "trait": "...", "polarity": "positive"|"negative", "confidence": 0.0-1.0, "reasoning": "brief explanation"}`;
  }

  /**
   * Parse AI classification response
   */
  private parseClassificationResponse(response: string): SignalClassification {
    try {
      // Extract JSON from response (handle markdown code blocks)
      const jsonMatch = response.match(/\{[\s\S]*\}/);
      if (!jsonMatch) {
        throw new Error('No JSON found in response');
      }

      const parsed = JSON.parse(jsonMatch[0]);

      return {
        isSignal: Boolean(parsed.isSignal),
        signalType: this.validateSignalType(parsed.signalType),
        trait: this.validateTrait(parsed.trait),
        polarity: parsed.polarity === 'positive' ? 'positive' : 'negative',
        confidence: Math.max(0, Math.min(1, Number(parsed.confidence) || 0.5)),
        reasoning: String(parsed.reasoning || '')
      };
    } catch (error) {
      console.error('[SignalDetector] Failed to parse AI response:', error);
      return {
        isSignal: false,
        signalType: 'none',
        trait: TRAIT_TYPES.TONE_AND_VOICE,
        polarity: 'negative',
        confidence: 0,
        reasoning: 'Parse error'
      };
    }
  }

  /**
   * Validate signal type from AI response
   */
  private validateSignalType(type: string): SignalType {
    const valid: SignalType[] = ['correction', 'approval', 'frustration', 'explicit_feedback', 'none'];
    return valid.includes(type as SignalType) ? (type as SignalType) : 'none';
  }

  /**
   * Validate trait from AI response
   */
  private validateTrait(trait: string): TraitType {
    const validTraits = Object.values(TRAIT_TYPES);
    return validTraits.includes(trait as any) ? trait : TRAIT_TYPES.TONE_AND_VOICE;
  }

  /**
   * Infer trait from message content (simple keyword-based)
   */
  private inferTraitFromContent(text: string): TraitType {
    const lower = text.toLowerCase();

    if (/\b(wrong|incorrect|false|error|mistake|actually)\b/.test(lower)) {
      return TRAIT_TYPES.DOMAIN_EXPERTISE;
    }
    if (/\b(logic|reasoning|explain|why|how|step)\b/.test(lower)) {
      return TRAIT_TYPES.REASONING_STYLE;
    }
    if (/\b(rude|polite|helpful|listen|understand)\b/.test(lower)) {
      return TRAIT_TYPES.SOCIAL_DYNAMICS;
    }
    if (/\b(creative|original|boring|interesting)\b/.test(lower)) {
      return TRAIT_TYPES.CREATIVE_EXPRESSION;
    }

    return TRAIT_TYPES.TONE_AND_VOICE;
  }

  /**
   * Build training context from conversation history
   */
  private buildContext(
    userMessage: ChatMessageEntity,
    aiMessage: ChatMessageEntity | null,
    history: ChatMessageEntity[]
  ): string {
    const contextParts: string[] = [];

    // Include relevant history (last 3-5 messages)
    const relevantHistory = history.slice(-5);
    for (const msg of relevantHistory) {
      if (msg.id === userMessage.id || (aiMessage && msg.id === aiMessage.id)) continue;
      const role = msg.senderType === 'human' ? 'user' : 'assistant';
      contextParts.push(`<|${role}|>\n${msg.content?.text || ''}`);
    }

    if (aiMessage) {
      contextParts.push(`<|assistant|>\n${aiMessage.content?.text || ''}`);
    }
    contextParts.push(`<|user|>\n${userMessage.content?.text || ''}`);

    return contextParts.join('\n\n');
  }

  /**
   * Check for repeated questions (frustration indicator)
   */
  checkForRepetition(
    userMessage: ChatMessageEntity,
    recentUserMessages: ChatMessageEntity[]
  ): boolean {
    const currentText = (userMessage.content?.text || '').toLowerCase().trim();
    if (currentText.length < 10) return false;

    for (const msg of recentUserMessages) {
      if (msg.id === userMessage.id || msg.senderId !== userMessage.senderId) continue;

      const previousText = (msg.content?.text || '').toLowerCase().trim();
      if (this.calculateSimilarity(currentText, previousText) > 0.7) {
        return true;
      }
    }

    return false;
  }

  /**
   * Jaccard similarity between two texts
   */
  private calculateSimilarity(text1: string, text2: string): number {
    const words1 = new Set(text1.split(/\s+/).filter(w => w.length > 2));
    const words2 = new Set(text2.split(/\s+/).filter(w => w.length > 2));

    if (words1.size === 0 || words2.size === 0) return 0;

    let intersection = 0;
    for (const word of words1) {
      if (words2.has(word)) intersection++;
    }

    return intersection / (words1.size + words2.size - intersection);
  }
}

// Singleton
let _signalDetector: SignalDetector | null = null;

export function getSignalDetector(): SignalDetector {
  if (!_signalDetector) {
    _signalDetector = new SignalDetector();
  }
  return _signalDetector;
}
