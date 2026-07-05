/**
 * Voice Configuration
 *
 * Centralized config for TTS/STT with easy adapter swapping.
 * Adapter names MUST match Rust adapter name() returns exactly.
 *
 * Model metadata (HF repos, sample rates, capabilities) lives in Constants.ts TTS_MODELS.
 * This file handles runtime configuration and user preferences.
 */

import { TTS_MODELS, STT_MODELS } from '@system/shared/Constants';

// TTS Adapter Constants — derived from centralized TTS_MODELS
export const TTS_ADAPTERS = {
  KOKORO: TTS_MODELS.KOKORO.id,
  PIPER: TTS_MODELS.PIPER.id,
  EDGE: TTS_MODELS.EDGE.id,
  POCKET: TTS_MODELS.POCKET.id,
  ORPHEUS: TTS_MODELS.ORPHEUS.id,
  SILENCE: TTS_MODELS.SILENCE.id,
} as const;

export type TTSAdapter = typeof TTS_ADAPTERS[keyof typeof TTS_ADAPTERS];

// STT Adapter Constants — derived from centralized STT_MODELS
export const STT_ADAPTERS = {
  WHISPER: STT_MODELS.WHISPER.id,
  MOONSHINE: STT_MODELS.MOONSHINE.id,
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
      pocket?: {
        voice: string;        // e.g., 'alba', or WAV file path for cloning
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
    adapter: TTS_ADAPTERS.EDGE,  // Edge TTS <200ms, concurrent, 300+ voices — best for live calls

    adapters: {
      kokoro: {
        voice: TTS_MODELS.KOKORO.defaultVoice,
        speed: 1.0,
      },
      piper: {
        voice: TTS_MODELS.PIPER.defaultVoice,
        speed: 1.0,
      },
      edge: {
        voice: TTS_MODELS.EDGE.defaultVoice,
      },
      pocket: {
        voice: TTS_MODELS.POCKET.defaultVoice,
      },
      orpheus: {
        voice: TTS_MODELS.ORPHEUS.defaultVoice,
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
 * Get TTS model metadata by adapter name.
 * Returns the centralized model info from Constants.ts.
 */
export function getTTSModelInfo(adapter: TTSAdapter) {
  const key = Object.keys(TTS_MODELS).find(
    k => TTS_MODELS[k as keyof typeof TTS_MODELS].id === adapter
  ) as keyof typeof TTS_MODELS | undefined;

  return key ? TTS_MODELS[key] : undefined;
}

/**
 * Get all TTS adapters that support a capability.
 */
export function getTTSAdaptersWithCapability(capability: 'voiceCloning' | 'emotionTags' | 'loraTrainable'): TTSAdapter[] {
  return Object.values(TTS_MODELS)
    .filter(m => m[capability])
    .map(m => m.id as TTSAdapter);
}

/**
 * Get all local (offline) TTS adapters.
 */
export function getLocalTTSAdapters(): TTSAdapter[] {
  return Object.values(TTS_MODELS)
    .filter(m => !m.requiresInternet)
    .map(m => m.id as TTSAdapter);
}
