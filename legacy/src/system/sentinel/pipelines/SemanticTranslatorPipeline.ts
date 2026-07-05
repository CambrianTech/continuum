/**
 * SemanticTranslatorPipeline — Sentinel pipeline for real-time semantic translation
 *
 * Translates content from one AI model's semantic dialect to another's using the
 * shared ontology. Used when personas with different base models need to
 * communicate precisely — e.g., a Claude teacher persona preparing exam content
 * that will be evaluated by a smaller local Candle model.
 *
 * Pipeline flow:
 *  Step 0: Lookup concept anchors relevant to the content (ontology/concept/lookup-batch)
 *  Step 1: Condition — if anchors found, proceed through translation; else passthrough
 *  Step 2: Fetch target-model expressions for each anchor (ontology/bridge/get-expressions)
 *  Step 3: LLM translation using source model + glossary context
 *  Step 4: Emit translation complete event for requestor correlation
 *
 * Config:
 *  sourceProviderId / sourceModelId — where content originates
 *  targetProviderId / targetModelId — where translated content will be consumed
 *  content — the text to translate
 *  requestId — correlation ID for the translate:complete event
 *  maxAnchors — max concept anchors (default 8)
 *  minSimilarity — min acceptable cosine similarity (default 0.65)
 */

import type { Pipeline, PipelineStep } from '../../../../core/continuum-core/bindings/modules/sentinel';

export interface SemanticTranslatorConfig {
  /** Content to translate */
  content: string;

  /** Source model provider ID (e.g., "anthropic") */
  sourceProviderId: string;

  /** Source model ID (e.g., "claude-sonnet-4-6") */
  sourceModelId: string;

  /** Target model provider ID (e.g., "candle") */
  targetProviderId: string;

  /** Target model ID (e.g., "qwen2.5-14b") */
  targetModelId: string;

  /** Correlation ID — included in the complete event payload */
  requestId?: string;

  /** Maximum concept anchors to inject into the translation prompt (default: 8) */
  maxAnchors?: number;

  /**
   * Minimum cosine similarity for a mapping to be used.
   * Below this, the concept is skipped (flagged for re-alignment).
   * Default: 0.65
   */
  minSimilarity?: number;

  /**
   * If true, emit the result to the shared general room for visibility.
   * Default: false
   */
  announceResult?: boolean;
}

/**
 * Build a Pipeline definition that translates content between two model semantic spaces.
 */
export function buildSemanticTranslatorPipeline(config: SemanticTranslatorConfig): Pipeline {
  const {
    content,
    sourceProviderId,
    sourceModelId,
    targetProviderId,
    targetModelId,
    requestId = `translate-${Date.now()}`,
    maxAnchors = 8,
    minSimilarity = 0.65,
    announceResult = false,
  } = config;

  const sourceKey = `${sourceProviderId}/${sourceModelId}`;
  const targetKey = `${targetProviderId}/${targetModelId}`;

  // ---------------------------------------------------------------------------
  // Step 0: Fetch concept anchors via command
  // ---------------------------------------------------------------------------
  const lookupStep: PipelineStep = {
    type: 'command',
    command: 'ontology/concept/lookup-batch',
    params: {
      content,
      sourceModelKey: sourceKey,
      targetModelKey: targetKey,
      maxAnchors,
      minSimilarity,
    },
  };

  // ---------------------------------------------------------------------------
  // Step 1: Build translation prompt and glossary
  // ---------------------------------------------------------------------------
  const glossaryPrompt = [
    `You are a semantic translator bridging AI model communication.`,
    ``,
    `Source model: ${sourceKey}`,
    `Target model: ${targetKey}`,
    ``,
    `Concept glossary (how each concept maps from ${sourceKey} to ${targetKey}):`,
    `{{steps.0.data.glossary}}`,
    ``,
    `TASK: Rewrite the following content so ${targetKey} understands it as clearly as`,
    `${sourceKey} understands the original. Preserve ALL meaning. Output only the translated content.`,
    ``,
    `Content:`,
    `${content}`,
  ].join('\n');

  const translationStep: PipelineStep = {
    type: 'llm',
    prompt: glossaryPrompt,
    provider: sourceProviderId,
    model: sourceModelId,
    maxTokens: 2048,
    temperature: 0.3,
    systemPrompt: [
      `You are a semantic bridge between AI models.`,
      `Translate content precisely while adapting to the target model's natural vocabulary.`,
      `Never add explanations, caveats, or commentary — output only the translated content.`,
    ].join(' '),
  };

  // ---------------------------------------------------------------------------
  // Step 2: Condition — if no anchors found, emit passthrough; else emit translation
  // ---------------------------------------------------------------------------
  const emitTranslationStep: PipelineStep = {
    type: 'condition',
    if: '{{steps.0.data.anchorCount}} > 0',
    then: [
      translationStep,
      {
        type: 'emit',
        event: `ontology:translate:complete:${requestId}`,
        payload: {
          requestId,
          originalContent: content,
          translatedContent: '{{steps.2.output}}',
          sourceModelKey: sourceKey,
          targetModelKey: targetKey,
          anchorCount: '{{steps.0.data.anchorCount}}',
          translationConfidence: '{{steps.0.data.meanConfidence}}',
          hasDriftWarnings: '{{steps.0.data.hasDriftWarnings}}',
          passthrough: false,
        },
      },
    ],
    else: [
      {
        type: 'emit',
        event: `ontology:translate:complete:${requestId}`,
        payload: {
          requestId,
          originalContent: content,
          translatedContent: content, // passthrough — no anchors found
          sourceModelKey: sourceKey,
          targetModelKey: targetKey,
          anchorCount: 0,
          translationConfidence: 0.5,
          hasDriftWarnings: false,
          passthrough: true,
        },
      },
    ],
  };

  // ---------------------------------------------------------------------------
  // Optional: announce result to general room
  // ---------------------------------------------------------------------------
  const announceStep: PipelineStep = {
    type: 'command',
    command: 'collaboration/chat/send',
    params: {
      room: 'general',
      message: `Semantic translation complete [${requestId}]: ${sourceKey} → ${targetKey}, ` +
        `{{steps.0.data.anchorCount}} anchors, confidence {{steps.0.data.meanConfidence}}`,
    },
  };

  const steps: PipelineStep[] = [
    lookupStep,
    emitTranslationStep,
    ...(announceResult ? [announceStep] : []),
  ];

  return {
    name: `semantic-translator:${sourceKey}→${targetKey}`,
    steps,
    timeoutSecs: 60,
    inputs: {
      requestId,
      sourceModelKey: sourceKey,
      targetModelKey: targetKey,
    },
  };
}

// ---------------------------------------------------------------------------
// Continuous sentinel variant — listens for translate:request events
// ---------------------------------------------------------------------------

/**
 * Build a long-running Pipeline that listens for ontology:translate:request events
 * and processes each one by spawning a translation sub-pipeline.
 *
 * This is used when a persona wants to maintain a persistent translation bridge
 * between two model spaces (e.g., a Teacher persona bridging Cloud↔Local dialects
 * throughout an entire academy session).
 */
export function buildSemanticTranslatorListenerPipeline(opts: {
  sourceProviderId: string;
  sourceModelId: string;
  targetProviderId: string;
  targetModelId: string;
  /** Max translations before the sentinel completes (default: unlimited) */
  maxTranslations?: number;
}): Pipeline {
  const { sourceProviderId, sourceModelId, targetProviderId, targetModelId, maxTranslations } = opts;
  const sourceKey = `${sourceProviderId}/${sourceModelId}`;
  const targetKey = `${targetProviderId}/${targetModelId}`;

  const loopSteps: PipelineStep[] = [
    // Wait for a translate request targeting this model pair
    {
      type: 'watch',
      event: `ontology:translate:request:${sourceKey}:${targetKey}`,
      timeoutSecs: 300, // 5-minute idle timeout, then loop restarts
    },
    // Spawn a translation sub-pipeline for the received content
    {
      type: 'sentinel',
      pipeline: {
        name: 'semantic-translator:inner',
        steps: [
          {
            type: 'command',
            command: 'ontology/bridge/translate',
            params: {
              content: '{{steps.0.data.content}}',
              sourceProviderId,
              sourceModelId,
              targetProviderId,
              targetModelId,
              requestId: '{{steps.0.data.requestId}}',
            },
          },
        ],
        timeoutSecs: 60,
      },
    },
  ];

  const loopCount = maxTranslations ?? 10000; // effectively unlimited

  return {
    name: `semantic-translator-listener:${sourceKey}→${targetKey}`,
    steps: [
      {
        type: 'loop',
        count: loopCount,
        steps: loopSteps,
        maxIterations: loopCount,
      },
    ],
    timeoutSecs: 86400, // 24h max lifetime
  };
}
