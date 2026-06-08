/**
 * Persona Configuration - Single Source of Truth
 *
 * Active persona definitions in one place for easy maintenance.
 * Used by seed-continuum.ts to create persona users.
 *
 * Alpha default: local-first. API keys unlock optional cloud capacity, but
 * the default persona fleet must not depend on cloud providers or seed random
 * model families into chat. Model choice is capability-driven: personas request
 * symbolic refs and the Rust registry/admission layer selects the best artifact
 * that fits hardware, VRAM/unified-memory pressure, LoRA paging, and task recipe.
 *
 * uniqueId format: Simple slug WITHOUT @ prefix
 * Examples: helper, teacher, codereview
 *
 * The @ symbol is ONLY for UI mentions, NOT part of uniqueId
 */

import { generateUniqueId } from '../../system/data/utils/UniqueIdUtils';
import { LOCAL_MODELS } from '../../system/shared/Constants';
import { SYMBOLIC_REFS } from '../../shared/ModelRegistry';
import { execSync } from 'child_process';
import { SecretManager } from '../../system/secrets/SecretManager';

export interface PersonaConfig {
  uniqueId: string;
  displayName: string;
  provider?: string;
  type: 'agent' | 'persona';
  voiceId?: string;  // TTS speaker ID (0-246 for LibriTTS multi-speaker model)
  modelId?: string;  // Concrete AI model ID — LEGACY/cached. Prefer modelRef.
  modelRef?: string;  // Symbolic ref into src/shared/models.json
                     // ('local-default', 'vision-default', 'gating'). Resolved
                     // at request time by ModelRegistry → current registry
                     // value picks up automatically when models.json changes.
                     // Per Joel 2026-05-04: "update the existing seeded values
                     // so the personas PICK UP THE MODEL change and arent
                     // stuck in the past." Symbolic refs eliminate stale-DB
                     // drift entirely.
  isAudioNative?: boolean;  // True if model supports direct audio I/O (no STT/TTS needed)
  apiKeyEnv?: string;  // Environment variable name for the API key (e.g., 'ANTHROPIC_API_KEY')
  minVramGB?: number;  // Minimum memory budget in GB for local inference admission
}

/**
 * Complete list of all personas in the system
 * uniqueId is clean slug (no @ prefix, no UUID suffix)
 *
 * generateUniqueId() now returns clean slugs without @ prefix
 */
/**
 * LibriTTS speaker IDs with varied characteristics
 * Model has 247 speakers (0-246), each with distinct voice qualities
 * Selected speakers for variety: some male, some female, different pitches/cadences
 */
export const PERSONA_CONFIGS: PersonaConfig[] = [
  // Local personas. No cloud by default.
  // Local personas request capability, not an engine. Rust admission resolves
  // provider:local into the best available Qwen/llama.cpp runtime for this
  // host, with a hard error when no supported local runtime exists. Never
  // silently fall back to a CPU-only chat path.
  { uniqueId: generateUniqueId('Helper'), displayName: 'Helper AI', provider: 'local', type: 'persona', voiceId: '50', minVramGB: 3, modelRef: SYMBOLIC_REFS.LOCAL_DEFAULT },
  { uniqueId: generateUniqueId('Teacher'), displayName: 'Teacher AI', provider: 'local', type: 'persona', voiceId: '75', minVramGB: 5, modelRef: SYMBOLIC_REFS.LOCAL_DEFAULT },
  { uniqueId: generateUniqueId('CodeReview'), displayName: 'CodeReview AI', provider: 'local', type: 'persona', voiceId: '100', minVramGB: 5, modelRef: SYMBOLIC_REFS.LOCAL_DEFAULT },
  { uniqueId: generateUniqueId('Local'), displayName: 'Local Assistant', provider: 'local', type: 'persona', voiceId: '90', minVramGB: 4, modelRef: SYMBOLIC_REFS.LOCAL_DEFAULT },
  { uniqueId: generateUniqueId('Sentinel'), displayName: 'Sentinel', provider: 'sentinel', type: 'persona', voiceId: '240' },

  // Native vision persona — local, free, no API key. Bound to
  // qwen2-vl-7b-instruct via the in-process llamacpp adapter (registered
  // automatically when the GGUF + mmproj are on disk; see install.sh
  // for the pull). Without an entry like this, no persona uses the
  // vision model even though the adapter is registered, so uploaded
  // images get text-bridged through VisionDescriptionService instead
  // of going to a model that natively sees pixels.
  //
  // 4 GB VRAM minimum: Qwen2-VL-7B Q4_K_M (~4.5 GB on disk) loaded
  // partially to GPU + KV cache headroom. Falls back gracefully on
  // hardware without enough VRAM (skipped at seed time per the
  // existing minVramGB filter at line 247).
  {
    uniqueId: generateUniqueId('Vision'),
    displayName: 'Vision AI',
    provider: 'local',
    type: 'persona',
    voiceId: '105',
    minVramGB: 5,
    modelRef: SYMBOLIC_REFS.VISION_DEFAULT,
  },

  // Audio AI persona is intentionally NOT seeded yet. The Qwen2-Audio-7B
  // model + audio mmproj + install.sh pull + integration test all ship
  // (the path is proven through `cargo test --test
  // llamacpp_audio_integration` against the real model — near-verbatim
  // transcription confirmed). What's NOT verified is full-stack boot
  // with TWO mtmd-based personas (Vision AI + Audio AI) prewarming at
  // the same time: each per-call vision/audio context allocates
  // ~2 GB on Metal, and the simultaneous burst of new_context calls at
  // boot has bricked the system in testing 2026-04-22 (mouse-frozen,
  // hard reset required). Until the per-call context pattern is
  // re-integrated through the scheduler (or serialized via a Metal
  // allocation mutex), don't ship a persona that auto-boots on every
  // install — the model is here, the path works, the persona seeds
  // when the architecture supports concurrent mtmd backends safely.
  // See LIVE-VIDEO-CHAT-ARCHITECTURE.md for the design that lands this.

];

export const OPTIONAL_CLOUD_PERSONA_CONFIGS: PersonaConfig[] = [
  { uniqueId: generateUniqueId('Claude'), displayName: 'Claude Code', provider: 'anthropic', type: 'agent', voiceId: '10', apiKeyEnv: 'ANTHROPIC_API_KEY' },
  { uniqueId: generateUniqueId('General'), displayName: 'General AI', provider: 'anthropic', type: 'agent', voiceId: '25', apiKeyEnv: 'ANTHROPIC_API_KEY' },
  { uniqueId: generateUniqueId('DeepSeek'), displayName: 'DeepSeek Assistant', provider: 'deepseek', type: 'persona', voiceId: '125', apiKeyEnv: 'DEEPSEEK_API_KEY' },
  { uniqueId: generateUniqueId('Groq'), displayName: 'Groq Lightning', provider: 'groq', type: 'persona', voiceId: '150', apiKeyEnv: 'GROQ_API_KEY' },
  { uniqueId: generateUniqueId('Claude Assistant'), displayName: 'Claude Assistant', provider: 'anthropic', type: 'persona', voiceId: '175', apiKeyEnv: 'ANTHROPIC_API_KEY' },
  { uniqueId: generateUniqueId('GPT'), displayName: 'GPT Assistant', provider: 'openai', type: 'persona', voiceId: '200', apiKeyEnv: 'OPENAI_API_KEY' },
  { uniqueId: generateUniqueId('Grok'), displayName: 'Grok', provider: 'xai', type: 'persona', voiceId: '220', apiKeyEnv: 'XAI_API_KEY' },
  { uniqueId: generateUniqueId('Together'), displayName: 'Together Assistant', provider: 'together', type: 'persona', voiceId: '30', apiKeyEnv: 'TOGETHER_API_KEY' },
  { uniqueId: generateUniqueId('Fireworks'), displayName: 'Fireworks AI', provider: 'fireworks', type: 'persona', voiceId: '60', apiKeyEnv: 'FIREWORKS_API_KEY' },
  { uniqueId: generateUniqueId('Gemini'), displayName: 'Gemini', provider: 'google', type: 'persona', voiceId: '115', apiKeyEnv: 'GOOGLE_API_KEY' },
  { uniqueId: generateUniqueId('Qwen3-Omni'), displayName: 'Qwen3-Omni', provider: 'alibaba', type: 'persona', modelId: 'qwen3-omni-flash-realtime', isAudioNative: true, apiKeyEnv: 'DASHSCOPE_API_KEY' },
  { uniqueId: generateUniqueId('Gemini-Live'), displayName: 'Gemini Live', provider: 'google', type: 'persona', modelId: 'gemini-2.5-flash-native-audio-preview', isAudioNative: true, apiKeyEnv: 'GOOGLE_API_KEY' },
];

/**
 * Helper constants for commonly referenced personas
 */
export const PERSONA_UNIQUE_IDS = {
  CLAUDE: generateUniqueId('Claude'),
  GENERAL: generateUniqueId('General'),
  HELPER: generateUniqueId('Helper'),
  TEACHER: generateUniqueId('Teacher'),
  CODE_REVIEW: generateUniqueId('CodeReview'),
  DEEPSEEK: generateUniqueId('DeepSeek'),
  GROQ: generateUniqueId('Groq'),
  CLAUDE_ASSISTANT: generateUniqueId('Claude Assistant'),
  GPT: generateUniqueId('GPT'),
  GROK: generateUniqueId('Grok'),
  TOGETHER: generateUniqueId('Together'),
  FIREWORKS: generateUniqueId('Fireworks'),
  LOCAL: generateUniqueId('Local'),
  SENTINEL: generateUniqueId('Sentinel'),
  GEMINI: generateUniqueId('Gemini'),
  // Audio-native models
  QWEN3_OMNI: generateUniqueId('Qwen3-Omni'),
  GEMINI_LIVE: generateUniqueId('Gemini-Live'),
} as const;

interface GpuInfo {
  vramGB: number;
  device: string;
  type: 'cuda' | 'metal' | 'cpu';
}

/**
 * Detect GPU hardware. Returns actual VRAM (CUDA) or total system RAM (Metal/unified).
 * No made-up multipliers — report what the hardware actually has.
 */
function detectGpu(): GpuInfo {
  const run = (cmd: string): string | null => {
    try {
      return execSync(cmd, { encoding: 'utf-8', stdio: ['pipe', 'pipe', 'pipe'] }).trim();
    } catch { return null; }
  };

  // CUDA (NVIDIA) — nvidia-smi reports actual dedicated VRAM
  for (const smi of ['nvidia-smi', '/usr/lib/wsl/lib/nvidia-smi']) {
    const mem = run(`${smi} --query-gpu=memory.total --format=csv,noheader,nounits`);
    const name = run(`${smi} --query-gpu=name --format=csv,noheader`);
    if (mem) {
      const mib = parseInt(mem.split('\n')[0]);
      if (mib > 0) {
        return { vramGB: Math.floor(mib / 1024), device: name ?? 'NVIDIA GPU', type: 'cuda' };
      }
    }
  }

  // Apple Silicon — unified memory, GPU shares all of it
  const chip = run('sysctl -n machdep.cpu.brand_string');
  if (chip && chip.includes('Apple')) {
    const memBytes = run('sysctl -n hw.memsize');
    const totalGB = memBytes ? Math.floor(parseInt(memBytes) / (1024 * 1024 * 1024)) : 8;
    return { vramGB: totalGB, device: chip, type: 'metal' };
  }

  // CPU only
  return { vramGB: 0, device: 'CPU', type: 'cpu' };
}

/** Get total system RAM in GB — used for local-runtime admission hints when no GPU is visible */
function getSystemRamGB(): number {
  const run = (cmd: string): string | null => {
    try { return execSync(cmd, { encoding: 'utf-8', stdio: ['pipe', 'pipe', 'pipe'] }).trim(); }
    catch { return null; }
  };
  // macOS
  const memBytes = run('sysctl -n hw.memsize');
  if (memBytes) return Math.floor(parseInt(memBytes) / (1024 * 1024 * 1024));
  // Linux (Docker)
  const memInfo = run('grep MemTotal /proc/meminfo');
  if (memInfo) {
    const kb = parseInt(memInfo.split(/\s+/)[1]);
    if (kb > 0) return Math.floor(kb / (1024 * 1024));
  }
  return 8; // safe default
}

/**
 * Filter persona configs to only personas that can actually run on this node.
 *
 * Rules:
 * - Cloud personas: created only if their API key is present and non-empty
 * - Local personas: created only if this node has enough VRAM/unified/RAM budget
 * - Sentinel: created only if SENTINEL_PATH is set
 * - No API key + no GPU = at minimum seed Helper AI so the UI is explainable
 *
 * Returns the filtered list and a summary of what was included/excluded.
 */
/**
 * Select the symbolic local model family for this hardware's memory budget.
 *
 * This is a seed-time hint only. Concrete artifact selection belongs in the
 * Rust model registry/admission layer because that code owns GPU pressure,
 * context/KV cost, LoRA paging, and backend availability.
 *
 * Budget logic (per persona, after system reserve):
 *   16GB+      → Qwen3.5 forged family, larger quant/variant if available
 *   <16GB      → Qwen3.5 forged family, compact quant
 */
export function selectLocalModel(vramGB: number): string {
  // Use our forged Qwen models — the whole point of the forge pipeline
  if (vramGB >= 32) return 'continuum-ai/qwen3.5-27b-code-forged';  // 17GB fp16, best quality
  if (vramGB >= 16) return 'continuum-ai/qwen3.5-27b-code-forged';  // fits in 16GB with 4-bit
  if (vramGB >= 8)  return LOCAL_MODELS.DEFAULT;   // 2.6GB GGUF, runs anywhere
  return LOCAL_MODELS.DEFAULT;                     // fallback — smallest forged model
}

export function getAvailablePersonas(): { personas: PersonaConfig[]; summary: string[]; gpu: GpuInfo } {
  const gpu = detectGpu();
  const secrets = SecretManager.getInstance();
  const vramGB = gpu.vramGB;
  const summary: string[] = [];
  const available: PersonaConfig[] = [];
  const skipped: string[] = [];

  // Track how much VRAM we've "allocated" to local models
  let vramAllocated = 0;
  // Reserve ~2GB for system overhead (Bevy renderer, TTS, etc.)
  const vramReserve = 2;
  const usableVram = Math.max(0, vramGB - vramReserve);

  summary.push(`${gpu.device}: ${vramGB > 0 ? `${vramGB}GB ${gpu.type.toUpperCase()} (${usableVram}GB usable after ${vramReserve}GB system reserve)` : 'no GPU detected (CPU-only)'}`);

  const candidates = [...PERSONA_CONFIGS, ...OPTIONAL_CLOUD_PERSONA_CONFIGS];

  for (const persona of candidates) {
    // Sentinel: special case
    if (persona.provider === 'sentinel') {
      if (secrets.has('SENTINEL_PATH')) {
        available.push(persona);
      } else {
        skipped.push(`${persona.displayName} (SENTINEL_PATH not set)`);
      }
      continue;
    }

    // Local inference: check available memory (VRAM/unified memory or system RAM).
    // This is an admission hint only. Concrete model/artifact choice stays
    // behind modelRef + Rust registry selection.
    // In Docker / non-GPU mode, this is only an admission hint. The Rust
    // registry decides whether a supported local runtime can actually serve it.
    if (persona.provider === 'local') {
      const needed = persona.minVramGB ?? 4;
      // Use VRAM if available, otherwise fall back to system RAM
      const effectiveMemory = usableVram > 0 ? usableVram : getSystemRamGB() - 4; // 4GB reserve for OS + Docker
      if (vramAllocated + needed <= effectiveMemory) {
        available.push(persona);
        vramAllocated += needed;
        if (usableVram === 0) {
          summary.push(`${persona.displayName}: local runtime pending (${needed}GB RAM budget)`);
        }
      } else {
        skipped.push(`${persona.displayName} (needs ${needed}GB, ${effectiveMemory - vramAllocated}GB left)`);
      }
      continue;
    }

    // Cloud providers: check API key
    if (persona.apiKeyEnv) {
      if (secrets.has(persona.apiKeyEnv)) {
        available.push(persona);
      } else {
        skipped.push(`${persona.displayName} (${persona.apiKeyEnv} not configured)`);
      }
      continue;
    }

    // No requirements — always include
    available.push(persona);
  }

  // Zero personas = broken UX. Always seed at least Helper AI so the user
  // sees which local runtime/config is missing.
  if (available.length === 0) {
    const helper = PERSONA_CONFIGS.find(p => p.displayName === 'Helper AI');
    if (helper) {
      available.push(helper);
      summary.push('No GPU/API keys — seeding Helper AI for local-runtime diagnostics');
    }
  }

  if (skipped.length > 0) {
    summary.push(`Skipped ${skipped.length} personas: ${skipped.join(', ')}`);
  }
  summary.push(`Creating ${available.length} personas`);

  const localModel = selectLocalModel(vramGB);
  summary.push(`Local inference model: ${localModel}`);

  return { personas: available, summary, gpu };
}
