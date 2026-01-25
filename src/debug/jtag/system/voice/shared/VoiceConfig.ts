/**
 * Voice Configuration
 *
 * Centralized config for TTS/STT with easy adapter swapping.
 *
 * Quality tiers:
 * - local: Fast, free, robotic (Piper, Kokoro)
 * - api: High quality, paid (ElevenLabs, Azure, Google)
 * - hybrid: Local backup with API fallback
 */

export type TTSAdapter =
  // Local (Free)
  | 'piper'        // Current default - basic quality, fast
  | 'kokoro'       // Better quality local option
  | 'silence'      // Testing only

  // API (Paid - Future)
  | 'elevenlabs'   // Premium quality, $$$
  | 'azure'        // Good quality, $
  | 'google'       // Good quality, $

  // System (Fallback)
  | 'macos-say';   // macOS native (same quality as Piper currently)

export type STTAdapter =
  | 'whisper'      // Local, good quality
  | 'deepgram'     // API, excellent quality (future)
  | 'azure';       // API, good quality (future)

export interface VoiceConfig {
  tts: {
    defaultAdapter: TTSAdapter;
    fallbackAdapter: TTSAdapter;

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
    defaultAdapter: STTAdapter;
    fallbackAdapter: STTAdapter;
  };

  // Performance
  maxSynthesisTimeMs: number;  // Timeout before fallback
  streamingEnabled: boolean;   // Stream audio chunks vs batch
}

// Default configuration (easily overrideable)
export const DEFAULT_VOICE_CONFIG: VoiceConfig = {
  tts: {
    defaultAdapter: 'piper',
    fallbackAdapter: 'macos-say',

    adapters: {
      piper: {
        voice: 'af',    // Female American English
        speed: 1.0,
      },
    },
  },

  stt: {
    defaultAdapter: 'whisper',
    fallbackAdapter: 'whisper',
  },

  maxSynthesisTimeMs: 5000,  // 5s timeout
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
 * Falls back to system defaults if user has no preferences
 */
export function getVoiceConfigForUser(
  userId: string,
  userPrefs?: UserVoicePreferences
): VoiceConfig {
  const config = { ...DEFAULT_VOICE_CONFIG };

  if (userPrefs?.preferredTTSAdapter) {
    config.tts.defaultAdapter = userPrefs.preferredTTSAdapter;
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
