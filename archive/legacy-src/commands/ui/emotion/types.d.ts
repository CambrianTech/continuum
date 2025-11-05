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
    target?: string;
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
    };
    error?: string;
}
export type EmotionType = 'fleeting' | 'contextual' | 'system';
export declare const VALID_EMOTIONS: readonly ["love", "joy", "excitement", "surprised", "grateful", "playful", "amazed", "thinking", "curious", "focused", "confused", "sleepy", "error", "warning", "offline", "connecting", "processing", "success"];
export type ValidEmotion = typeof VALID_EMOTIONS[number];
//# sourceMappingURL=types.d.ts.map