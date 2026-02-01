/**
 * CodingModelSelector - Routes coding tasks to appropriate frontier models
 *
 * Coding requires frontier models (Claude, GPT, DeepSeek) — not local Ollama.
 * This selector maps task types to model tiers:
 *
 * | Task Type    | Model Tier     | Why                                    |
 * |-------------|----------------|----------------------------------------|
 * | Planning    | Best reasoning | Architecture decisions need deep thought|
 * | Generation  | Best coding    | Writing code needs strong coding models |
 * | Editing     | Best coding    | Modifying code needs precision          |
 * | Review      | Any frontier   | Analysis is broadly capable             |
 * | Quick fix   | Fast + cheap   | Typos and simple fixes                  |
 * | Discovery   | Fast + cheap   | Codebase exploration is simple          |
 *
 * Provider fallback: anthropic → openai → deepseek → groq
 */

import type { CodingTaskType, CodingModelTier } from '../shared/CodingTypes';
import { MODEL_IDS } from '../../shared/Constants';
import { SOTA_PROVIDERS } from '../../user/server/config/PersonaModelConfigs';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('CodingModelSelector', 'code');

/**
 * Default model tiers for each task type.
 * Ordered by preference — first available provider wins.
 */
const DEFAULT_TIERS: Record<CodingTaskType, CodingModelTier> = {
  planning: {
    taskType: 'planning',
    provider: 'anthropic',
    model: MODEL_IDS.ANTHROPIC.SONNET_4_5,
    temperature: 0.3,
    maxTokens: 4000,
    description: 'Planning/architecture — best reasoning model',
  },
  generation: {
    taskType: 'generation',
    provider: 'anthropic',
    model: MODEL_IDS.ANTHROPIC.SONNET_4_5,
    temperature: 0.4,
    maxTokens: 4000,
    description: 'Code generation — strong coding model',
  },
  editing: {
    taskType: 'editing',
    provider: 'anthropic',
    model: MODEL_IDS.ANTHROPIC.SONNET_4_5,
    temperature: 0.2,
    maxTokens: 4000,
    description: 'Code editing — precise, low temperature',
  },
  review: {
    taskType: 'review',
    provider: 'deepseek',
    model: MODEL_IDS.DEEPSEEK.CHAT,
    temperature: 0.3,
    maxTokens: 3000,
    description: 'Code review — any frontier model works',
  },
  'quick-fix': {
    taskType: 'quick-fix',
    provider: 'groq',
    model: MODEL_IDS.GROQ.LLAMA_3_1_70B,
    temperature: 0.2,
    maxTokens: 2000,
    description: 'Quick fixes — fast and cheap',
  },
  discovery: {
    taskType: 'discovery',
    provider: 'groq',
    model: MODEL_IDS.GROQ.LLAMA_3_1_8B,
    temperature: 0.1,
    maxTokens: 1000,
    description: 'Discovery — codebase exploration, fast',
  },
};

/**
 * Provider fallback order when preferred provider is unavailable.
 * Prioritizes SOTA providers with strong coding capabilities.
 */
const PROVIDER_FALLBACK_ORDER: readonly string[] = [
  'anthropic',
  'openai',
  'deepseek',
  'xai',
  'google',
  'groq',
  'together',
  'fireworks',
] as const;

/**
 * Fallback models per provider (when the preferred model isn't available).
 */
const FALLBACK_MODELS: Record<string, string> = {
  'anthropic': MODEL_IDS.ANTHROPIC.SONNET_4_5,
  'openai': MODEL_IDS.OPENAI.GPT_4,
  'deepseek': MODEL_IDS.DEEPSEEK.CHAT,
  'groq': MODEL_IDS.GROQ.LLAMA_3_1_70B,
  'xai': MODEL_IDS.XAI.GROK_4,
  'google': 'gemini-2.0-flash',
  'together': MODEL_IDS.TOGETHER.LLAMA_3_1_70B,
  'fireworks': MODEL_IDS.FIREWORKS.LLAMA_3_1_70B,
};

export class CodingModelSelector {
  private _availableProviders: Set<string>;

  /**
   * @param availableProviders - Set of provider names that are currently registered and healthy.
   *                             Pass SOTA_PROVIDERS for production, or a subset for testing.
   */
  constructor(availableProviders?: Set<string>) {
    this._availableProviders = availableProviders ?? new Set(SOTA_PROVIDERS);
  }

  /**
   * Update the set of available providers (e.g., after health check).
   */
  set availableProviders(providers: Set<string>) {
    this._availableProviders = providers;
  }

  /**
   * Select the best model tier for a given task type.
   * Falls through provider fallback order if preferred provider is unavailable.
   */
  select(taskType: CodingTaskType): CodingModelTier {
    const defaultTier = DEFAULT_TIERS[taskType];

    // Try the default provider first
    if (this._availableProviders.has(defaultTier.provider)) {
      log.debug(`Selected ${defaultTier.provider}/${defaultTier.model} for ${taskType}`);
      return defaultTier;
    }

    // Fallback through provider order
    for (const provider of PROVIDER_FALLBACK_ORDER) {
      if (this._availableProviders.has(provider)) {
        const model = FALLBACK_MODELS[provider];
        const fallbackTier: CodingModelTier = {
          ...defaultTier,
          provider,
          model,
          description: `${defaultTier.description} (fallback: ${provider})`,
        };
        log.debug(`Fallback: ${provider}/${model} for ${taskType} (preferred ${defaultTier.provider} unavailable)`);
        return fallbackTier;
      }
    }

    // Last resort — return default tier anyway, let AIProviderDaemon handle the error
    log.warn(`No SOTA provider available for ${taskType}, using default tier (may fail)`);
    return defaultTier;
  }

  /**
   * Check if any frontier model is available for coding tasks.
   */
  get hasFrontierModel(): boolean {
    return PROVIDER_FALLBACK_ORDER.some(p => this._availableProviders.has(p));
  }

  /**
   * Get all configured tiers (for debugging/reporting).
   */
  get allTiers(): readonly CodingModelTier[] {
    return Object.values(DEFAULT_TIERS);
  }
}
