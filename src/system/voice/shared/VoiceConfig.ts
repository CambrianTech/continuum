/**
 * Voice Configuration
 *
 * Centralized config for TTS/STT with easy adapter swapping.
 * Adapter names MUST match Rust adapter name() returns exactly.
 *
 * TTS: Kokoro (primary), Edge (cloud), Orpheus (expressive), Piper, Silence
 * STT: Whisper (primary), Moonshine (fast), OpenAI Realtime, Stub
 */

// TTS Adapter Constants — names MUST match Rust adapter name() returns
export const TTS_ADAPTERS = {
  KOKORO: 'kokoro',
  PIPER: 'piper',
  EDGE: 'edge',          // Rust msedge-tts crate (free Microsoft neural voices)
  ORPHEUS: 'orpheus',    // Candle GGUF Llama-3B (expressive, emotion tags)
  SILENCE: 'silence',
} as const;

export type TTSAdapter = typeof TTS_ADAPTERS[keyof typeof TTS_ADAPTERS];

// STT Adapter Constants — names MUST match Rust adapter name() returns
export const STT_ADAPTERS = {
  WHISPER: 'whisper',
  MOONSHINE: 'moonshine',  // ONNX, sub-100ms, great for live transcription
  OPENAI_REALTIME: 'openai-realtime',
  STUB: 'stub',
} as const;

export type STTAdapter = typeof STT_ADAPTERS[keyof typeof STT_ADAPTERS];

export interface VoiceConfig {
  tts: {
    adapter: TTSAdapter;  // NO FALLBACKS - fail if this doesn't work

    // Per-adapter config
    adapters: {
      kokoro?: {
        voice: string;        // e.g., 'af' (default female), 'am_adam', 'bf_emma'
        speed: number;        // 0.5-2.0
      };
      piper?: {
        voice: string;        // e.g., 'af' (default female)
        speed: number;        // 0.5-2.0
      };
      edge?: {
        voice: string;        // e.g., 'en-US-AriaNeural'
      };
      orpheus?: {
        voice: string;        // e.g., 'tara', 'leo', 'zoe' (8 built-in voices)
      };
    };
  };

  stt: {
    adapter: STTAdapter;  // NO FALLBACKS - fail if this doesn't work
  };

  // Performance
  streamingEnabled: boolean;   // Stream audio chunks vs batch
  // Note: TTS timeout is activity-based (VoiceService two-phase strategy),
  // not a static limit. Synthesis runs as long as the adapter is active.
}

// Default configuration (easily overrideable)
export const DEFAULT_VOICE_CONFIG: VoiceConfig = {
  tts: {
    adapter: TTS_ADAPTERS.KOKORO,  // Kokoro 82M — fast, high quality, local

    adapters: {
      kokoro: {
        voice: 'af',    // American Female (default)
        speed: 1.0,
      },
      piper: {
        voice: 'af',    // Fallback
        speed: 1.0,
      },
    },
  },

  stt: {
    adapter: STT_ADAPTERS.WHISPER,  // Use constants, NO fallbacks
  },

  streamingEnabled: false,    // Batch mode for now
};

// Per-user voice preferences (future)
export interface UserVoicePreferences {
  userId: string;
  preferredTTSAdapter?: TTSAdapter;
  preferredVoice?: string;
  speechRate?: number;  // 0.5-2.0
}

/**
 * Get voice config for a user
 * Uses system defaults if user has no preferences
 */
export function getVoiceConfigForUser(
  userId: string,
  userPrefs?: UserVoicePreferences
): VoiceConfig {
  const config = { ...DEFAULT_VOICE_CONFIG };

  if (userPrefs?.preferredTTSAdapter) {
    config.tts.adapter = userPrefs.preferredTTSAdapter;
  }

  if (userPrefs?.speechRate) {
    if (config.tts.adapters.kokoro) {
      config.tts.adapters.kokoro.speed = userPrefs.speechRate;
    }
    if (config.tts.adapters.piper) {
      config.tts.adapters.piper.speed = userPrefs.speechRate;
    }
  }

  return config;
}

/**
 * Adapter comparison (all registered in Rust TTS registry):
 *
 * Tier 1 (Fast, local):
 * - Kokoro: 82M ONNX, ~97ms TTFB, 80.9% TTS Arena — PRIMARY
 * - Edge: Microsoft neural voices, <200ms, free cloud, no API key
 *
 * Tier 2 (Expressive, local):
 * - Orpheus: 3B Candle GGUF, emotion tags <laugh> <sigh>, ~2-5s CPU
 *
 * Tier 3 (Functional):
 * - Piper: ONNX, ~42s, local
 * - Silence: Testing only (produces zeros)
 */
