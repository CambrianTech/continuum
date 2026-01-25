/**
 * Voice Configuration
 *
 * Centralized config for TTS/STT with easy adapter swapping.
 *
 * Quality tiers:
 * - local: Fast, free, robotic (Piper, Kokoro)
 * - api: High quality, paid (ElevenLabs, Azure, Google)
 */

// TTS Adapter Constants
export const TTS_ADAPTERS = {
  PIPER: 'piper',
  KOKORO: 'kokoro',
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
      piper?: {
        voice: string;        // e.g., 'af' (default female)
        speed: number;        // 0.5-2.0
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
  maxSynthesisTimeMs: number;  // Timeout before failure
  streamingEnabled: boolean;   // Stream audio chunks vs batch
}

// Default configuration (easily overrideable)
export const DEFAULT_VOICE_CONFIG: VoiceConfig = {
  tts: {
    adapter: TTS_ADAPTERS.PIPER,  // Use constants, NO fallbacks

    adapters: {
      piper: {
        voice: 'af',    // Female American English
        speed: 1.0,
      },
    },
  },

  stt: {
    adapter: STT_ADAPTERS.WHISPER,  // Use constants, NO fallbacks
  },

  maxSynthesisTimeMs: 5000,  // 5s timeout before FAILURE (not fallback)
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

  if (userPrefs?.speechRate && config.tts.adapters.piper) {
    config.tts.adapters.piper.speed = userPrefs.speechRate;
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
 * Tier 2 (Good, affordable):
 * - Kokoro: 80.9% TTS Arena win rate, free local
 * - Google Cloud: Good quality, $
 *
 * Tier 3 (Functional, free):
 * - Piper: Basic quality, fast, free local (CURRENT)
 * - macOS say: Basic quality, free system
 *
 * Recommendation: Start with Piper, upgrade to Kokoro or ElevenLabs
 * when quality matters (demos, production).
 */
