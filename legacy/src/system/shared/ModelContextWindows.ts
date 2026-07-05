/**
 * Model Context / Speed Helpers — Shim over the adapter authority
 *
 * @deprecated The functions in this file are a transitional shim. The adapter
 * is the authority on its own models — it declares ModelInfo (context window,
 * tokens/sec, capabilities) via the `ai/model-info` IPC and registers them in
 * `ModelRegistry`. Callers should read those fields directly through PRG-injected
 * `ModelInfo` rather than calling these helpers. When all 18 call sites migrate,
 * this file goes away entirely.
 *
 * What changed in this revision:
 *
 *   - Deleted the `MODEL_CONTEXT_WINDOWS` / `MODEL_INFERENCE_SPEEDS` static
 *     lookup tables (~150 lines of hardcoded model knowledge that drifted
 *     against reality every time a forge run shipped a new artifact).
 *   - All functions now delegate to `ModelRegistry` — populated at boot from
 *     each provider's `ai/model-info` IPC. When the registry doesn't know
 *     a model, callers get a sane default + a one-shot warn-log (visibility
 *     into "the adapter for this model isn't reporting its own ModelInfo").
 *
 * Why this matters: the static tables encoded "what we believed about a model
 * in early 2026." Forged Qwen3.5-4B-code shipped with a 262144-token context;
 * the table didn't have an entry → caller saw 8192 default → RAG truncated
 * pointlessly. Adapter authority + warn-log makes that class of bug visible
 * AND fixable in one place (the adapter's ModelInfo).
 *
 * Removed in this PR (smell that needs to die):
 *
 *   - `MODEL_CONTEXT_WINDOWS` and `MODEL_INFERENCE_SPEEDS` literal maps —
 *     ~70 entries, all guesses; ModelRegistry has the live truth.
 *   - The < 500 TPS heuristic in `isSlowLocalModel` — replaced by a real
 *     check on whether the registry classifies the model as local.
 *   - Per-function string-table normalization (suffix-strip, prefix-match) —
 *     ModelRegistry already does this, and once was enough.
 */

import { ModelRegistry } from './ModelRegistry';

/**
 * Default context window for unknown models. Conservative — if the adapter
 * authority doesn't know the model, we stay safe rather than overflow.
 */
export const DEFAULT_CONTEXT_WINDOW = 8192;

/**
 * Default inference speed when the registry hasn't been told. Conservative
 * "local medium" guess; correct values come from the adapter's ModelInfo.
 */
export const DEFAULT_INFERENCE_SPEED = 80;

/**
 * Default target latency in seconds for inference. Used by callers that
 * compute latency-aware token budgets.
 */
export const DEFAULT_TARGET_LATENCY_SECONDS = 30;

/** Track which models we've already warned about so we log once, not per-call. */
const _warnedMissingModels = new Set<string>();
function _warnMissing(kind: 'contextWindow' | 'tokensPerSecond', model: string, provider?: string) {
  const key = `${kind}:${provider ?? '*'}:${model}`;
  if (_warnedMissingModels.has(key)) return;
  _warnedMissingModels.add(key);
  // Use console; this file is shared (browser + server). Verbose only on first miss.
  // eslint-disable-next-line no-console
  console.warn(
    `[ModelContextWindows] No ${kind} for model="${model}" provider="${provider ?? '*'}" — ` +
    `using default. Adapter should report this via ai/model-info IPC into ModelRegistry.`
  );
}

/**
 * Get inference speed for a model in tokens per second.
 *
 * Source of truth: ModelRegistry (populated from each adapter's ModelInfo
 * via the `ai/model-info` IPC). When unknown, returns DEFAULT_INFERENCE_SPEED
 * and warns once so the missing entry is diagnosable.
 *
 * @deprecated Read `ModelInfo.tokensPerSecond` from the PRG-injected struct
 * directly. This shim exists for backward compatibility while callers migrate.
 */
export function getInferenceSpeed(model: string, provider?: string): number {
  const tps = ModelRegistry.sharedInstance().tokensPerSecond(model, provider);
  if (tps !== undefined) return tps;
  _warnMissing('tokensPerSecond', model, provider);
  return DEFAULT_INFERENCE_SPEED;
}

/**
 * Calculate maximum input tokens based on target latency.
 * Derived from `getInferenceSpeed` × `targetLatencySeconds`.
 *
 * @deprecated Compute inline from `ModelInfo.tokensPerSecond` at the callsite.
 */
export function getLatencyAwareTokenLimit(
  model: string,
  targetLatencySeconds: number = DEFAULT_TARGET_LATENCY_SECONDS,
  provider?: string
): number {
  return Math.floor(targetLatencySeconds * getInferenceSpeed(model, provider));
}

/**
 * Check if a model is a local model (needs latency-aware budgeting).
 *
 * Replaces the < 500 TPS heuristic with the honest signal: does the
 * adapter's reported `isLocal` capability say so? When the registry doesn't
 * know, falls back to "is the speed slow" because that's still a reasonable
 * proxy and matches the previous behavior on unknown models.
 *
 * @deprecated Read `ModelInfo.isLocal` directly at the callsite.
 */
export function isSlowLocalModel(model: string, provider?: string): boolean {
  const registry = ModelRegistry.sharedInstance();
  const meta = registry.get(model, provider);
  if (meta?.isLocal !== undefined) return meta.isLocal;
  // Unknown model — fall back to the speed heuristic (preserves previous behavior
  // while we wait for adapters to finish reporting `isLocal` in their ModelInfo).
  return getInferenceSpeed(model, provider) < 500;
}

/**
 * Get context window size for a model.
 *
 * Source of truth: ModelRegistry. When the registry doesn't know, returns
 * DEFAULT_CONTEXT_WINDOW (conservative 8192) and warns once so the missing
 * entry is diagnosable.
 *
 * @deprecated Read `ModelInfo.contextWindow` from the PRG-injected struct
 * directly.
 */
export function getContextWindow(model: string, provider?: string): number {
  const ctx = ModelRegistry.sharedInstance().contextWindow(model, provider);
  if (ctx !== undefined) return ctx;
  _warnMissing('contextWindow', model, provider);
  return DEFAULT_CONTEXT_WINDOW;
}

/**
 * Check if a model has a large context window (>32K).
 * Useful for deciding whether to include more context in RAG.
 *
 * @deprecated Compute inline: `modelInfo.contextWindow > 32768`.
 */
export function isLargeContextModel(modelId: string, provider?: string): boolean {
  return getContextWindow(modelId, provider) > 32768;
}

/**
 * Get recommended output token budget for a model.
 * 25% of context for small windows, capped at 4K for large.
 *
 * @deprecated Compute inline from `ModelInfo.contextWindow`.
 */
export function getRecommendedMaxOutputTokens(modelId: string, provider?: string): number {
  const contextWindow = getContextWindow(modelId, provider);
  if (contextWindow <= 8192) return Math.floor(contextWindow * 0.25);
  return Math.min(4096, Math.floor(contextWindow * 0.1));
}

/**
 * Calculate available tokens for input given output reservation.
 *
 * @deprecated Compute inline from `ModelInfo.contextWindow`.
 */
export function getAvailableInputTokens(
  modelId: string,
  reservedOutputTokens: number,
  safetyMargin: number = 500,
  provider?: string
): number {
  const contextWindow = getContextWindow(modelId, provider);
  return Math.max(0, contextWindow - reservedOutputTokens - safetyMargin);
}
