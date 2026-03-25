/**
 * TaskAwareProviderRouter — Per-task model routing for PersonaUser inference
 *
 * The persona is the IDENTITY (personality, LoRA adapters, memory).
 * The model should be selected PER TASK based on what's available
 * and what the task demands.
 *
 * When a persona's default provider is local (candle) and the task
 * requires capabilities the local model lacks (tool use, code generation,
 * complex reasoning), this router upgrades to the best available cloud
 * provider. The persona's identity stays the same — only the compute changes.
 *
 * Task domains that REQUIRE cloud models:
 * - code/debug/analysis: Local 3B can't write code or use tools
 * - tool_use: Local 3B ignores XML tool format entirely
 *
 * Task domains that work fine locally:
 * - conversation/social: Local 3B handles chat adequately
 * - creative: Acceptable for simple creative tasks
 *
 * @see #371 — Per-task model routing
 */

import { MODEL_IDS } from '../../../shared/Constants';
import { Logger } from '../../../core/logging/Logger';
import { AIProvidersStatus } from '../../../../commands/ai/providers/status/shared/AIProvidersStatusTypes';

const log = Logger.create('TaskAwareProviderRouter', 'persona');

/** Domains where local models are completely inadequate */
const CLOUD_REQUIRED_DOMAINS = new Set([
  'code', 'debug', 'analysis', 'tool_use',
]);

/** Provider fallback order for capability-demanding tasks */
const CLOUD_PROVIDER_FALLBACK: readonly string[] = [
  'deepseek',    // Best price/performance for coding
  'anthropic',   // Best reasoning
  'openai',      // Strong general
  'groq',        // Fast
  'xai',         // Capable
  'google',      // Capable
  'together',    // Open models
  'fireworks',   // Open models
] as const;

/** Default model per cloud provider */
const CLOUD_PROVIDER_MODELS: Record<string, string> = {
  'deepseek': MODEL_IDS.DEEPSEEK.CHAT,
  'anthropic': MODEL_IDS.ANTHROPIC.SONNET_4_5,
  'openai': MODEL_IDS.OPENAI.GPT_4,
  'groq': MODEL_IDS.GROQ.LLAMA_3_1_70B,
  'xai': MODEL_IDS.XAI.GROK_4,
  'google': 'gemini-2.0-flash',
  'together': MODEL_IDS.TOGETHER.LLAMA_3_1_70B,
  'fireworks': MODEL_IDS.FIREWORKS.LLAMA_3_1_70B,
};

// ============================================================================
// Provider Discovery (cached)
// ============================================================================

let _cachedProviders: Set<string> | null = null;
let _cacheTimestamp = 0;
const CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes

/**
 * Get the set of currently configured cloud providers.
 * Cached for 5 minutes to avoid querying on every inference request.
 */
export async function getAvailableCloudProviders(): Promise<Set<string>> {
  const now = Date.now();
  if (_cachedProviders && (now - _cacheTimestamp) < CACHE_TTL_MS) {
    return _cachedProviders;
  }

  try {
    const result = await AIProvidersStatus.execute({});
    const configured = new Set<string>();
    for (const p of result.providers) {
      if (p.isConfigured && p.category === 'cloud') {
        configured.add(p.provider);
      }
    }
    _cachedProviders = configured;
    _cacheTimestamp = now;
    log.info(`Provider cache refreshed: ${configured.size} cloud providers available [${[...configured].join(', ')}]`);
    return configured;
  } catch (err) {
    log.warn(`Failed to query provider status: ${err}`);
    // Return empty set — system will stay local (zero API keys scenario)
    return _cachedProviders ?? new Set();
  }
}

/** Result of task-aware routing — may override model and provider */
export interface TaskRoutingResult {
  /** Model to use (may differ from persona's default) */
  model: string;
  /** Provider to use (may differ from persona's default) */
  provider: string;
  /** Whether the provider was upgraded from the persona's default */
  upgraded: boolean;
  /** Reason for the routing decision */
  reason: string;
}

/**
 * Determine if the persona's default provider needs upgrading for this task.
 *
 * @param defaultModel - Persona's configured model
 * @param defaultProvider - Persona's configured provider
 * @param taskDomain - Classified domain from Rust (e.g., 'code', 'creative', 'support')
 * @param hasTools - Whether tools are being injected into the request
 * @param availableProviders - Set of registered provider IDs from the adapter registry
 */
export function routeForTask(
  defaultModel: string,
  defaultProvider: string,
  taskDomain: string | undefined,
  hasTools: boolean,
  availableProviders: Set<string>,
): TaskRoutingResult {
  const isLocalProvider = defaultProvider === 'candle' || defaultProvider === 'candle-quantized';

  // If the persona already uses a cloud provider, no routing needed
  if (!isLocalProvider) {
    return {
      model: defaultModel,
      provider: defaultProvider,
      upgraded: false,
      reason: `Provider ${defaultProvider} is already cloud-capable`,
    };
  }

  // Check if this task domain requires cloud capabilities
  const domainRequiresCloud = taskDomain && CLOUD_REQUIRED_DOMAINS.has(taskDomain);
  const toolsRequireCloud = hasTools; // Local 3B ignores tools entirely

  if (!domainRequiresCloud && !toolsRequireCloud) {
    return {
      model: defaultModel,
      provider: defaultProvider,
      upgraded: false,
      reason: `Domain '${taskDomain ?? 'unknown'}' works with local model`,
    };
  }

  // Need cloud — find the best available provider
  for (const provider of CLOUD_PROVIDER_FALLBACK) {
    if (availableProviders.has(provider)) {
      const model = CLOUD_PROVIDER_MODELS[provider];
      const reason = domainRequiresCloud
        ? `Domain '${taskDomain}' requires cloud model — upgraded to ${provider}`
        : `Tools require cloud model (local 3B ignores XML tools) — upgraded to ${provider}`;

      log.info(reason);
      return { model, provider, upgraded: true, reason };
    }
  }

  // No cloud provider available — stay local and hope for the best
  // (This is the zero-API-keys scenario — system works but with degraded capability)
  return {
    model: defaultModel,
    provider: defaultProvider,
    upgraded: false,
    reason: `Domain '${taskDomain}' wants cloud but no cloud providers available — using local`,
  };
}
