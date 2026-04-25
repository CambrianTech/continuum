/**
 * Persona Configuration - Single Source of Truth
 *
 * All persona definitions in one place for easy maintenance.
 * Used by seed-continuum.ts to create persona users.
 *
 * Hardware-aware: getAvailablePersonas() filters based on:
 *   - API keys present in environment (cloud providers)
 *   - GPU VRAM available (local candle inference)
 *
 * uniqueId format: Simple slug WITHOUT @ prefix
 * Examples: claude, helper, grok, sentinel
 *
 * The @ symbol is ONLY for UI mentions, NOT part of uniqueId
 */

import { generateUniqueId } from '../../system/data/utils/UniqueIdUtils';
import { LOCAL_MODELS } from '../../system/shared/Constants';
import { execSync } from 'child_process';

export interface PersonaConfig {
  uniqueId: string;
  displayName: string;
  provider?: string;
  type: 'agent' | 'persona';
  voiceId?: string;  // TTS speaker ID (0-246 for LibriTTS multi-speaker model)
  modelId?: string;  // AI model ID (e.g., 'qwen3-omni-flash-realtime' for audio-native)
  isAudioNative?: boolean;  // True if model supports direct audio I/O (no STT/TTS needed)
  apiKeyEnv?: string;  // Environment variable name for the API key (e.g., 'ANTHROPIC_API_KEY')
  minVramGB?: number;  // Minimum VRAM in GB for local inference (candle provider)
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
  // Core agents (cloud — need API key)
  { uniqueId: generateUniqueId('Claude'), displayName: 'Claude Code', provider: 'anthropic', type: 'agent', voiceId: '10', apiKeyEnv: 'ANTHROPIC_API_KEY' },
  { uniqueId: generateUniqueId('General'), displayName: 'General AI', provider: 'anthropic', type: 'agent', voiceId: '25', apiKeyEnv: 'ANTHROPIC_API_KEY' },

  // Local personas (Candle native Rust inference — need GPU VRAM)
  // Model sizes: 14B coder ~9GB, 8B instruct ~5GB, 3B instruct ~3GB
  // On big GPUs (5090 32GB), we run specialized models per persona
  // On small GPUs (8GB), everyone shares the 3B model
  // Local personas: NO provider hardcode. The Rust AdapterRegistry routes
  // by honest model availability: DMR (Metal on Mac, CUDA on Linux/Nvidia)
  // when the model is pulled, llama-vulkan for other GPU hardware, hard
  // error if neither is available. Never silent Candle-CPU fallback.
  // 4B GGUF is the universal default — fits every supported machine, fast
  // on Metal/Vulkan/CUDA. Power users upgrade to 27B manually (HF-gated).
  { uniqueId: generateUniqueId('Helper'), displayName: 'Helper AI', provider: 'local', type: 'persona', voiceId: '50', minVramGB: 3, modelId: LOCAL_MODELS.DEFAULT },
  { uniqueId: generateUniqueId('Teacher'), displayName: 'Teacher AI', provider: 'local', type: 'persona', voiceId: '75', minVramGB: 5, modelId: LOCAL_MODELS.DEFAULT },
  { uniqueId: generateUniqueId('CodeReview'), displayName: 'CodeReview AI', provider: 'local', type: 'persona', voiceId: '100', minVramGB: 5, modelId: LOCAL_MODELS.DEFAULT },

  // Cloud provider personas (each needs its own API key)
  { uniqueId: generateUniqueId('DeepSeek'), displayName: 'DeepSeek Assistant', provider: 'deepseek', type: 'persona', voiceId: '125', apiKeyEnv: 'DEEPSEEK_API_KEY' },
  { uniqueId: generateUniqueId('Groq'), displayName: 'Groq Lightning', provider: 'groq', type: 'persona', voiceId: '150', apiKeyEnv: 'GROQ_API_KEY' },
  { uniqueId: generateUniqueId('Claude Assistant'), displayName: 'Claude Assistant', provider: 'anthropic', type: 'persona', voiceId: '175', apiKeyEnv: 'ANTHROPIC_API_KEY' },
  { uniqueId: generateUniqueId('GPT'), displayName: 'GPT Assistant', provider: 'openai', type: 'persona', voiceId: '200', apiKeyEnv: 'OPENAI_API_KEY' },
  { uniqueId: generateUniqueId('Grok'), displayName: 'Grok', provider: 'xai', type: 'persona', voiceId: '220', apiKeyEnv: 'XAI_API_KEY' },
  { uniqueId: generateUniqueId('Together'), displayName: 'Together Assistant', provider: 'together', type: 'persona', voiceId: '30', apiKeyEnv: 'TOGETHER_API_KEY' },
  { uniqueId: generateUniqueId('Fireworks'), displayName: 'Fireworks AI', provider: 'fireworks', type: 'persona', voiceId: '60', apiKeyEnv: 'FIREWORKS_API_KEY' },
  { uniqueId: generateUniqueId('Local'), displayName: 'Local Assistant', provider: 'local', type: 'persona', voiceId: '90', minVramGB: 4, modelId: LOCAL_MODELS.DEFAULT },
  { uniqueId: generateUniqueId('Sentinel'), displayName: 'Sentinel', provider: 'sentinel', type: 'persona', voiceId: '240' },
  { uniqueId: generateUniqueId('Gemini'), displayName: 'Gemini', provider: 'google', type: 'persona', voiceId: '115', apiKeyEnv: 'GOOGLE_API_KEY' },

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
    modelId: LOCAL_MODELS.VISION,
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

  // Audio-native personas (need specific API keys)
  {
    uniqueId: generateUniqueId('Qwen3-Omni'),
    displayName: 'Qwen3-Omni',
    provider: 'alibaba',
    type: 'persona',
    modelId: 'qwen3-omni-flash-realtime',
    isAudioNative: true,
    apiKeyEnv: 'DASHSCOPE_API_KEY',
  },
  {
    uniqueId: generateUniqueId('Gemini-Live'),
    displayName: 'Gemini Live',
    provider: 'google',
    type: 'persona',
    modelId: 'gemini-2.5-flash-native-audio-preview',
    isAudioNative: true,
    apiKeyEnv: 'GOOGLE_API_KEY',
  },
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

/** Get total system RAM in GB — used for CPU inference budget when no GPU */
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
 * Filter PERSONA_CONFIGS to only personas that can actually run on this hardware.
 *
 * Rules:
 * - Cloud personas: created only if their API key is set in environment
 * - Local (candle) personas: created only if GPU has enough VRAM
 * - Sentinel: created only if SENTINEL_PATH is set
 * - No API key + no GPU = at minimum create Helper AI with candle fallback (CPU mode)
 *
 * Returns the filtered list and a summary of what was included/excluded.
 */
/**
 * Select the best local model for this hardware's VRAM budget.
 * Returns HuggingFace model ID suitable for Candle inference.
 *
 * Budget logic (per persona, after system reserve):
 *   32GB+ CUDA → 14B coder (BF16 if available, else GGUF Q5)
 *   16-31GB    → 8B instruct
 *   8-15GB     → 3B instruct (default)
 *   <8GB       → 3B instruct (will be slow but works)
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

  for (const persona of PERSONA_CONFIGS) {
    // Sentinel: special case
    if (persona.provider === 'sentinel') {
      if (process.env.SENTINEL_PATH) {
        available.push(persona);
      } else {
        skipped.push(`${persona.displayName} (SENTINEL_PATH not set)`);
      }
      continue;
    }

    // Local candle inference: check available memory (VRAM or system RAM)
    // In Docker / CPU mode, Metal/CUDA aren't available — Candle uses system RAM.
    // A 4B Q4_K_M model needs ~3GB regardless of whether it's in VRAM or RAM.
    if (persona.provider === 'candle') {
      const needed = persona.minVramGB ?? 4;
      // Use VRAM if available, otherwise fall back to system RAM
      const effectiveMemory = usableVram > 0 ? usableVram : getSystemRamGB() - 4; // 4GB reserve for OS + Docker
      if (vramAllocated + needed <= effectiveMemory) {
        available.push(persona);
        vramAllocated += needed;
        if (usableVram === 0) {
          summary.push(`${persona.displayName}: CPU inference (${needed}GB RAM)`);
        }
      } else {
        skipped.push(`${persona.displayName} (needs ${needed}GB, ${effectiveMemory - vramAllocated}GB left)`);
      }
      continue;
    }

    // Cloud providers: check API key
    if (persona.apiKeyEnv) {
      if (process.env[persona.apiKeyEnv]) {
        available.push(persona);
      } else {
        skipped.push(`${persona.displayName} (${persona.apiKeyEnv} not set)`);
      }
      continue;
    }

    // No requirements — always include
    available.push(persona);
  }

  // Zero personas = broken UX. Always seed at least Helper AI so the user
  // sees a living system. CPU inference is slow but functional.
  if (available.length === 0) {
    const helper = PERSONA_CONFIGS.find(p => p.displayName === 'Helper AI');
    if (helper) {
      available.push(helper);
      summary.push('No GPU/API keys — seeding Helper AI for CPU inference (slow but functional)');
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
