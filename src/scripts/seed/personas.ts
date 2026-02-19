/**
 * Persona Configuration - Single Source of Truth
 *
 * All persona definitions in one place for easy maintenance.
 * Used by seed-continuum.ts to create persona users.
 *
 * uniqueId format: Simple slug WITHOUT @ prefix
 * Examples: claude, helper, grok, sentinel
 *
 * The @ symbol is ONLY for UI mentions, NOT part of uniqueId
 */

import { generateUniqueId } from '../../system/data/utils/UniqueIdUtils';

export interface PersonaConfig {
  uniqueId: string;
  displayName: string;
  provider?: string;
  type: 'agent' | 'persona';
  voiceId?: string;  // TTS speaker ID (0-246 for LibriTTS multi-speaker model)
  modelId?: string;  // AI model ID (e.g., 'qwen3-omni-flash-realtime' for audio-native)
  isAudioNative?: boolean;  // True if model supports direct audio I/O (no STT/TTS needed)
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
  // Core agents
  { uniqueId: generateUniqueId('Claude'), displayName: 'Claude Code', provider: 'anthropic', type: 'agent', voiceId: '10' },
  { uniqueId: generateUniqueId('General'), displayName: 'General AI', provider: 'anthropic', type: 'agent', voiceId: '25' },

  // Local personas (Candle native Rust inference)
  { uniqueId: generateUniqueId('Helper'), displayName: 'Helper AI', provider: 'candle', type: 'persona', voiceId: '50' },
  { uniqueId: generateUniqueId('Teacher'), displayName: 'Teacher AI', provider: 'candle', type: 'persona', voiceId: '75' },
  { uniqueId: generateUniqueId('CodeReview'), displayName: 'CodeReview AI', provider: 'candle', type: 'persona', voiceId: '100' },

  // Cloud provider personas
  { uniqueId: generateUniqueId('DeepSeek'), displayName: 'DeepSeek Assistant', provider: 'deepseek', type: 'persona', voiceId: '125' },
  { uniqueId: generateUniqueId('Groq'), displayName: 'Groq Lightning', provider: 'groq', type: 'persona', voiceId: '150' },
  { uniqueId: generateUniqueId('Claude Assistant'), displayName: 'Claude Assistant', provider: 'anthropic', type: 'persona', voiceId: '175' },
  { uniqueId: generateUniqueId('GPT'), displayName: 'GPT Assistant', provider: 'openai', type: 'persona', voiceId: '200' },
  { uniqueId: generateUniqueId('Grok'), displayName: 'Grok', provider: 'xai', type: 'persona', voiceId: '220' },
  { uniqueId: generateUniqueId('Together'), displayName: 'Together Assistant', provider: 'together', type: 'persona', voiceId: '30' },
  { uniqueId: generateUniqueId('Fireworks'), displayName: 'Fireworks AI', provider: 'fireworks', type: 'persona', voiceId: '60' },
  { uniqueId: generateUniqueId('Local'), displayName: 'Local Assistant', provider: 'candle', type: 'persona', voiceId: '90' },
  { uniqueId: generateUniqueId('Sentinel'), displayName: 'Sentinel', provider: 'sentinel', type: 'persona', voiceId: '240' },
  { uniqueId: generateUniqueId('Gemini'), displayName: 'Gemini', provider: 'google', type: 'persona', voiceId: '115' },

  // Audio-native personas (no STT/TTS needed - direct audio I/O)
  {
    uniqueId: generateUniqueId('Qwen3-Omni'),
    displayName: 'Qwen3-Omni',
    provider: 'alibaba',
    type: 'persona',
    modelId: 'qwen3-omni-flash-realtime',
    isAudioNative: true,
    // No voiceId - Qwen3-Omni has its own native voices (Cherry, Ethan, etc.)
  },
  {
    uniqueId: generateUniqueId('Gemini-Live'),
    displayName: 'Gemini Live',
    provider: 'google',
    type: 'persona',
    modelId: 'gemini-2.5-flash-native-audio-preview',
    isAudioNative: true,
    // No voiceId - Gemini has its own native voices (Aoede, Puck, etc.)
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
