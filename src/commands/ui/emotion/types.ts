/**
 * TypeScript types for Emotion Command system
 * Clean, typed interfaces for emotion handling
 */

export interface EmotionParams {
  feeling: string;
  intensity?: 'low' | 'medium' | 'high';
  duration?: number;
  persist?: boolean;
  target?: string;
}

export interface EmotionConfig {
  emoji: string;
  color: string;
  animation: string;
  target?: string | undefined;
  persistent: boolean;
  returnToHome: boolean;
  duration: number;
}

export interface EmotionDefinition {
  name: string;
  category: string;
  icon: string;
  description: string;
  params: string;
  examples: string[];
  usage: string;
}

export interface EmotionContext {
  webSocketServer?: any;
  continuonStatus?: any;
}

export interface EmotionResult {
  success: boolean;
  message: string;
  data?: {
    emotion: string;
    config: EmotionConfig;
    timestamp: string;
  } | undefined;
  error?: string | undefined;
}

export type EmotionType = 'fleeting' | 'contextual' | 'system';

export const VALID_EMOTIONS = [
  // Fleeting emotions (fade automatically)
  'love', 'joy', 'excitement', 'surprised', 'grateful', 'playful', 'amazed',
  // Contextual emotions (fade unless persist=true)
  'thinking', 'curious', 'focused', 'confused', 'sleepy',
  // System states (should usually persist)
  'error', 'warning', 'offline', 'connecting', 'processing', 'success'
] as const;

export type ValidEmotion = typeof VALID_EMOTIONS[number];