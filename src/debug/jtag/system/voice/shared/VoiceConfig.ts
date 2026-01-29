/**
 * Voice Configuration
 *
 * Centralized config for TTS/STT with easy adapter swapping.
 *
 * Quality tiers:
 * - local fast: Kokoro (82M, ONNX, ~97ms TTFB) — PRIMARY
 * - local slow: Piper (ONNX, ~42s) — fallback
 * - cloud free: Edge-TTS (Microsoft neural voices) — no API key
 * - cloud paid: ElevenLabs, Azure, Google — high quality
 */

// TTS Adapter Constants
export const TTS_ADAPTERS = {
  KOKORO: 'kokoro',
  PIPER: 'piper',
  EDGE_TTS: 'edge-tts',
  SILENCE: 'silence',
  ELEVENLABS: 'elevenlabs',
  AZURE: 'azure',
  GOOGLE: 'google',
} as const;

export type TTSAdapter = typeof TTS_ADAPTERS[keyof typeof TTS_ADAPTERS];

// STT Adapter Constants
export const STT_ADAPTERS = {
  WHISPER: 'whisper',
  DEEPGRAM: 'deepgram',
  AZURE: 'azure',
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
      'edge-tts'?: {
        voice: string;        // e.g., 'en-US-AriaNeural'
      };
      elevenlabs?: {
        apiKey?: string;
        voiceId: string;      // e.g., 'EXAVITQu4vr4xnSDxMaL' (Bella)
        model: string;        // e.g., 'eleven_turbo_v2'
      };
      azure?: {
        apiKey?: string;
        region: string;
        voice: string;
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
 * Quality comparison (based on TTS Arena rankings + real-world usage)
 *
 * Tier 1 (Natural, expensive):
 * - ElevenLabs Turbo v2: 80%+ win rate, $$$
 * - Azure Neural: Professional quality, $$
 *
 * Tier 2 (Good, local):
 * - Kokoro: 80.9% TTS Arena win rate, ~97ms TTFB, free local (CURRENT)
 * - Edge-TTS: Microsoft neural voices, free cloud, no API key
 *
 * Tier 3 (Functional, free):
 * - Piper: Basic quality, slow (~42s), free local (fallback)
 *
 * Current: Kokoro (primary) — fast, natural, local ONNX inference
 */
