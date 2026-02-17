/**
 * ModelProvider - Model configuration types for Sentinels
 *
 * ARCHITECTURE: All inference routes through the inference/generate command
 * via IPC. No direct model invocation here - just configuration types.
 */

/**
 * Model capacity levels - from tiny to state-of-the-art
 */
export enum ModelCapacity {
  TINY = 'tiny',           // <1B params - fastest, least capable
  SMALL = 'small',         // 1-3B params - fast, basic tasks
  MEDIUM = 'medium',       // 7-13B params - balanced
  LARGE = 'large',         // 30-70B params - high capability
  SOTA = 'sota',           // State of the art - best available
}

/**
 * Model providers
 */
export enum ModelProvider {
  LOCAL = 'local',         // Local inference service
  CANDLE = 'candle',       // Candle native Rust inference
  ANTHROPIC = 'anthropic', // Claude API
  OPENAI = 'openai',       // OpenAI API
  AUTO = 'auto',           // Auto-select best available
}

/**
 * Model selection config - passed to inference/generate command
 */
export interface ModelConfig {
  provider?: ModelProvider;
  capacity?: ModelCapacity;
  model?: string;          // Specific model override
  maxTokens?: number;
  temperature?: number;
}
