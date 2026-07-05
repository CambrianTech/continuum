/**
 * ModelRegistry — single source of truth reader for src/shared/models.json.
 *
 * ALL model lookups go through here. Consumers:
 *   - src/scripts/seed/personas.ts  (resolves persona.modelRef → current modelId)
 *   - Rust local runtime/admission code (accepts symbolic refs, resolves to concrete model)
 *   - src/scripts/download-models.sh (reads via jq for tier/auto_download set)
 *   - install.sh (reads via jq for PERSONA_MODEL tier resolution)
 *
 * Architectural rule: NEVER hardcode a model ID in code or DB rows. Always
 * use a symbolic ref ('local-default', 'vision-default', 'gating') OR a
 * registry key ('qwen3.5-4b-code-forged'). Registry edits propagate
 * everywhere on next read; seeded data does not need migration.
 */

import * as fs from 'fs';
import * as path from 'path';

export type ModelKind = 'chat-llm' | 'vision-llm' | 'embedding' | 'stt' | 'tts' | 'tts-trainable' | 'vad' | 'chat-llm-fast';

/**
 * Host-tier label that drives default-model selection. Most tiers are
 * RAM-bucketed (mba/mid/full); `mac_intel_discrete` is a hardware-shaped
 * override for Mac Intel hosts with a discrete AMD or integrated Intel
 * UHD Metal device — even with 32GB RAM, llama.cpp's Metal-AMD shader
 * path produces incoherent tokens (continuum 2026-05-30 evidence on
 * MacBookPro15,1 / Radeon Pro 560X), so the tier policy must override
 * the RAM-based bucket and pick the smallest forged model that CPU
 * inference can comfortably run. Matches the Rust `HwCapabilityTier`
 * variant `MacIntelMetalDiscrete` — keep the two in sync.
 */
export type Tier = 'mba' | 'mid' | 'full' | 'mac_intel_discrete';

/**
 * Canonical symbolic refs that personas store in DB. Code reads these
 * constants — never hardcode the underlying strings. Joel rule
 * 2026-05-04: "define constants not magic strings".
 *
 * Adding a new symbolic ref: add the constant here, add the entry to
 * src/shared/models.json `symbolic_refs{}`, document below.
 */
export const SYMBOLIC_REFS = {
  /** Local chat model — tier-resolved. Resolves to tiers[host_tier].default_chat. */
  LOCAL_DEFAULT: 'local-default',
  /** Native-vision model. Currently bound to qwen2-vl-7b. */
  VISION_DEFAULT: 'vision-default',
  /** Fast classification/gating model. */
  GATING: 'gating',
} as const;
export type SymbolicRef = typeof SYMBOLIC_REFS[keyof typeof SYMBOLIC_REFS];

/** Tier constants — code uses these instead of bare 'mba' / 'mid' / 'full' strings. */
export const TIERS = {
  MBA: 'mba' as const,
  MID: 'mid' as const,
  FULL: 'full' as const,
  MAC_INTEL_DISCRETE: 'mac_intel_discrete' as const,
};

export interface ModelSpec {
  kind: ModelKind;
  hf_repo: string;
  format: string;
  architecture?: string;
  files?: string[];
  size_gb: number;
  min_ram_gb?: number;
  chat_template?: string;
  description: string;
  auto_load?: boolean;
}

export interface TierSpec {
  min_ram_gb: number;
  default_chat: string;  // registry key
  description: string;
}

interface RegistryFile {
  models: Record<string, ModelSpec>;
  tiers: Record<Tier, TierSpec>;
  symbolic_refs: Record<string, { by_tier?: boolean; model?: string }>;
  personas: Record<string, string>;
  auto_download: {
    always: string[];
    by_tier: Record<Tier, string[]>;
  };
  chat_templates: Record<string, Record<string, string>>;
}

let _cached: RegistryFile | null = null;

function load(): RegistryFile {
  if (_cached) return _cached;
  // Resolve registry across three runtime shapes:
  //   1. Compiled: __dirname=dist/shared, JSON copied alongside by build script.
  //   2. tsx dev: __dirname=src/shared, JSON sits next to ModelRegistry.ts.
  //   3. dist-without-copy: __dirname=dist/shared, source JSON at ../../src/shared/.
  // Try each in order so the first one that exists wins. Surface a clear
  // error if none — no silent fallback to default model.
  const candidates = [
    path.join(__dirname, 'models.json'),
    path.join(__dirname, '..', '..', 'src', 'shared', 'models.json'),
    path.join(__dirname, '..', '..', '..', 'src', 'shared', 'models.json'),
  ];
  let found: string | undefined;
  for (const p of candidates) {
    if (fs.existsSync(p)) { found = p; break; }
  }
  if (!found) {
    throw new Error(
      `ModelRegistry: models.json not found. Tried: ${candidates.join(', ')}. ` +
      `Build script must copy shared/models.json → dist/shared/models.json.`
    );
  }
  const raw = fs.readFileSync(found, 'utf8');
  _cached = JSON.parse(raw) as RegistryFile;
  return _cached;
}

/**
 * Pick host tier from total RAM in GB. Same logic as install.sh's
 * tier-detection block — kept consistent so install-time and runtime
 * resolve to the same default model.
 *
 * Pure-RAM fallback. Prefer [`tierFromHost`] when a hardware-capability
 * hint is available — RAM alone misclassifies Mac Intel + discrete GPU
 * (32GB Mac Intel reads as "full" but its 4GB AMD VRAM can't run a 4B
 * model, and the Metal-AMD shader path is broken — continuum 2026-05-30
 * evidence).
 */
export function tierFromRamGB(ramGB: number): Tier {
  if (ramGB >= 32) return 'full';
  if (ramGB >= 24) return 'mid';
  return 'mba';
}

/**
 * Pick host tier from RAM + hardware-capability tier (matches the Rust
 * `HwCapabilityTier` variants from `cognition::model_resolver`). The
 * hardware tier overrides RAM when it names a class whose physical-VRAM
 * or shader-path budget diverges from the RAM-based expectation.
 *
 * Current overrides:
 * - `mac_intel_metal_discrete` → `mac_intel_discrete`. Mac Intel with
 *   discrete AMD or integrated Intel UHD. llama.cpp Metal shaders
 *   unreliable on this path; the tier maps to a small CPU-runnable
 *   model regardless of system RAM.
 *
 * Other hardware tiers (M-series, NVIDIA, VulkanAmd) fall through to
 * RAM-based selection — they have unified or reliable discrete VRAM
 * and the RAM heuristic remains accurate. Pass `hwTier === undefined`
 * to get pure-RAM behavior (equivalent to [`tierFromRamGB`]).
 */
export function tierFromHost(ramGB: number, hwTier?: string): Tier {
  if (hwTier === 'mac_intel_metal_discrete') return 'mac_intel_discrete';
  return tierFromRamGB(ramGB);
}

/**
 * Resolve a symbolic ref ('local-default', 'vision-default', 'gating') OR
 * a direct registry key to a concrete ModelSpec. Always reads current
 * registry — DB rows storing symbolic refs auto-pick-up registry edits.
 */
export function resolveModel(ref: string, tier?: Tier): ModelSpec {
  const reg = load();
  const sym = reg.symbolic_refs[ref];
  if (sym) {
    if (sym.by_tier) {
      if (!tier) {
        throw new Error(`Symbolic ref '${ref}' is tier-dependent but no tier provided.`);
      }
      const modelKey = reg.tiers[tier].default_chat;
      const spec = reg.models[modelKey];
      if (!spec) throw new Error(`Tier '${tier}' default_chat '${modelKey}' not found in models.`);
      return spec;
    }
    if (sym.model) {
      const spec = reg.models[sym.model];
      if (!spec) throw new Error(`Symbolic ref '${ref}' → '${sym.model}' not found in models.`);
      return spec;
    }
  }
  const direct = reg.models[ref];
  if (direct) return direct;
  throw new Error(`Model ref '${ref}' not found (not a symbolic ref nor a registry key).`);
}

/**
 * Resolve a persona's symbolic ref to a concrete model spec.
 * `personas.ts` stores symbolic refs in modelRef field; this function
 * is what the AI provider chain calls at request time.
 */
export function resolvePersonaModel(personaDisplayName: string, tier: Tier): ModelSpec {
  const reg = load();
  const ref = reg.personas[personaDisplayName];
  if (!ref) throw new Error(`No registry entry for persona '${personaDisplayName}'.`);
  return resolveModel(ref, tier);
}

/**
 * Set of model registry keys that should be downloaded by model-init for
 * a given tier. Used by download-models.sh and integration tests.
 */
export function downloadSetForTier(tier: Tier): string[] {
  const reg = load();
  return [...reg.auto_download.always, ...(reg.auto_download.by_tier[tier] || [])];
}

/**
 * Get all registered persona-displayName → symbolic-ref pairs. Reconciler
 * uses this on startup to ensure DB persona rows match current registry.
 */
export function allPersonaRefs(): Record<string, string> {
  return { ...load().personas };
}

/**
 * Get the symbolic ref a persona should store in DB.
 * Use this in seed-in-process.ts when creating/updating persona rows.
 */
export function symbolicRefForPersona(personaDisplayName: string): string | undefined {
  return load().personas[personaDisplayName];
}

export function getModelSpec(key: string): ModelSpec | undefined {
  return load().models[key];
}

export function getChatTemplate(name: string): Record<string, string> | undefined {
  return load().chat_templates[name];
}

/** Force re-read on next call (test helper). */
export function _resetCacheForTests(): void {
  _cached = null;
}
