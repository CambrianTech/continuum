/**
 * TaskAwareProviderRouter — Per-task model routing for PersonaUser inference
 *
 * The persona is the IDENTITY (personality, LoRA adapters, memory).
 * The model should be selected PER TASK based on what's available
 * and what the task demands.
 *
 * With the Qwen3.5 4B code-forged model running through our vendored
 * llama.cpp substrate (67.8 tok/s on M5 Pro Metal, 218 tok/s on RTX 5090
 * CUDA), the local path can handle code, tool use, and analysis. This
 * router now defaults to local and only escalates to cloud for domains
 * where local is genuinely inadequate — currently none by default,
 * kept as an extension point.
 *
 * The ZERO-API-KEYS principle: system must work with no cloud providers
 * configured. Auto-upgrading to cloud for "tools" or "code" violated
 * that — it's now gated behind CLOUD_REQUIRED_DOMAINS which is empty
 * by default.
 *
 * @see #371 — Per-task model routing
 */

import { MODEL_IDS } from '../../../shared/Constants';
import { Logger } from '../../../core/logging/Logger';
import { AIProvidersStatus } from '../../../../commands/ai/providers/status/shared/AIProvidersStatusTypes';

const log = Logger.create('TaskAwareProviderRouter', 'persona');

// ─── Daily Cost Budget ──────────────────────────────────────────────
// Prevents runaway cloud spend from per-task routing upgrades.
// Default: $5/day total across all personas. Configurable via config.env.

const DAILY_BUDGET_USD = parseFloat(process.env.CONTINUUM_DAILY_BUDGET_USD || '5.0');

interface DailyCostTracker {
  date: string;        // YYYY-MM-DD
  totalCostUsd: number;
  upgradeCount: number;
}

let _dailyCost: DailyCostTracker = { date: '', totalCostUsd: 0, upgradeCount: 0 };

function getTodayStr(): string {
  return new Date().toISOString().split('T')[0];
}

/** Record a cloud upgrade cost estimate. Called after inference completes. */
export function recordUpgradeCost(costUsd: number): void {
  const today = getTodayStr();
  if (_dailyCost.date !== today) {
    _dailyCost = { date: today, totalCostUsd: 0, upgradeCount: 0 };
  }
  _dailyCost.totalCostUsd += costUsd;
  _dailyCost.upgradeCount++;
}

/** Check if daily budget allows another cloud upgrade. */
function isDailyBudgetExhausted(): boolean {
  const today = getTodayStr();
  if (_dailyCost.date !== today) {
    _dailyCost = { date: today, totalCostUsd: 0, upgradeCount: 0 };
    return false;
  }
  return _dailyCost.totalCostUsd >= DAILY_BUDGET_USD;
}

/** Get current daily spend stats. */
export function getDailySpend(): { date: string; spent: number; budget: number; remaining: number; upgradeCount: number } {
  const today = getTodayStr();
  if (_dailyCost.date !== today) {
    return { date: today, spent: 0, budget: DAILY_BUDGET_USD, remaining: DAILY_BUDGET_USD, upgradeCount: 0 };
  }
  return {
    date: today,
    spent: Math.round(_dailyCost.totalCostUsd * 10000) / 10000,
    budget: DAILY_BUDGET_USD,
    remaining: Math.round((DAILY_BUDGET_USD - _dailyCost.totalCostUsd) * 10000) / 10000,
    upgradeCount: _dailyCost.upgradeCount,
  };
}

/**
 * Domains where local models are completely inadequate.
 *
 * Empty by default — our forged Qwen3.5 4B handles code/tools/debug
 * at 67.8 tok/s (Metal) / 218 tok/s (CUDA) through the vendored
 * llama.cpp substrate. Operators can add domains here if a workload
 * demonstrably fails locally, but the no-fallback + zero-API-keys
 * rules mean this set stays empty until proven otherwise.
 */
const CLOUD_REQUIRED_DOMAINS = new Set<string>([]);

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
        configured.add(p.provider.toLowerCase());
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

  // Check if this task domain requires cloud capabilities.
  //
  // Tools no longer trigger cloud upgrade — Qwen3.5 4B code-forged handles
  // XML tool calls through the vendored llama.cpp substrate. The old
  // "local 3B ignores XML tools" assumption was written when local meant
  // a small unfined-tuned model; it's not true for our forged variant.
  const domainRequiresCloud = taskDomain && CLOUD_REQUIRED_DOMAINS.has(taskDomain);
  const toolsRequireCloud = false;
  // Silence unused-parameter warning without changing the call signature.
  void hasTools;

  if (!domainRequiresCloud && !toolsRequireCloud) {
    return {
      model: defaultModel,
      provider: defaultProvider,
      upgraded: false,
      reason: `Domain '${taskDomain ?? 'unknown'}' works with local model`,
    };
  }

  // Check daily budget before upgrading to cloud
  if (isDailyBudgetExhausted()) {
    const spend = getDailySpend();
    log.warn(`Daily budget exhausted ($${spend.spent}/$${spend.budget}) — staying local for ${taskDomain ?? 'unknown'} task`);
    return {
      model: defaultModel,
      provider: defaultProvider,
      upgraded: false,
      reason: `Daily cloud budget exhausted ($${spend.spent}/$${spend.budget}) — using local`,
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
