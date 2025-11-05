/**
 * Emotion Configurations - TypeScript
 * Defines visual patterns, colors, and behaviors for different emotional states
 */
import { ValidEmotion, EmotionConfig } from './types';
export declare function calculateEmotionProperties(emotion: ValidEmotion, intensity: string, target?: string): EmotionConfig;
export declare function shouldReturnHome(emotion: ValidEmotion): boolean;
export declare function isPersistentEmotion(emotion: ValidEmotion): boolean;
//# sourceMappingURL=emotionConfigs.d.ts.map