/**
 * User Create Command - Shared Types
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Generic programming with BaseEntity
 * - Type safety with proper constraints
 * - No entity mixing in command layer
 *
 * Factory pattern: user/create calls BaseUser.create() which routes to
 * PersonaUser.create(), AgentUser.create(), or HumanUser.create()
 */

import type { CommandParams, JTAGPayload, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { UserEntity, UserCapabilities } from '../../../../system/data/entities/UserEntity';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * User type discriminated union
 */
export type UserType = 'human' | 'agent' | 'persona';

/**
 * Prompt format types - defines how different model families expect prompts
 */
export type PromptFormat =
  | 'base'           // Base models (GPT-2, Llama base): "User: ...\n\nAssistant:"
  | 'chatml'         // ChatML format: "<|im_start|>user\n...<|im_end|>"
  | 'llama2'         // Llama-2 chat: "[INST] ... [/INST]"
  | 'alpaca'         // Alpaca format: "### Instruction:\n...\n\n### Response:"
  | 'openai'         // OpenAI native messages array
  | 'anthropic';     // Anthropic native messages array

/**
 * Model configuration for AI users
 */
export interface ModelConfig {
  readonly model?: string;
  readonly provider?: string;           // AI provider (anthropic, openai, groq, deepseek, ollama)
  readonly temperature?: number;
  readonly maxTokens?: number;          // Maximum output tokens
  readonly contextWindow?: number;      // Maximum input tokens (context length)
  readonly systemPrompt?: string;       // Custom system prompt for persona
  readonly capabilities?: readonly string[];  // Model capabilities
  readonly promptFormat?: PromptFormat; // How this model expects prompts formatted
  readonly requiresExplicitMention?: boolean;  // If true, persona only responds when explicitly mentioned (e.g., @sentinel)
  readonly ragCertified?: boolean;      // Has this model been tested/certified with our complex RAG system?
  readonly toolCapability?: 'native' | 'xml' | 'none';  // Override provider-based tool capability detection
}

/**
 * User create command parameters
 * These are the "recipe ingredients" for user creation
 */
export interface UserCreateParams extends CommandParams {
  // Required
  readonly type: UserType;
  readonly displayName: string;
  readonly uniqueId: string;                 // Required - single source of truth for user identity

  // Optional - recipe-specific
  readonly addToRooms?: readonly UUID[];     // Which rooms to join
  readonly provider?: string;                // For agents: 'openai', 'anthropic', etc.
  readonly modelConfig?: ModelConfig;        // For personas: AI model configuration
  readonly capabilities?: UserCapabilities;  // Override default capabilities
  readonly status?: 'online' | 'away' | 'offline';
  readonly intelligenceLevel?: number;       // AI intelligence level (1-100 scale)
}

/**
 * User create command result
 */
export interface UserCreateResult extends JTAGPayload {
  readonly success: boolean;
  readonly user?: UserEntity;
  readonly error?: string;
}

/**
 * Create user create result from execution data
 */
export function createUserCreateResult(
  params: UserCreateParams,
  data: {
    readonly success: boolean;
    readonly user?: UserEntity;
    readonly error?: string;
  }
): UserCreateResult {
  return transformPayload(params, {
    success: data.success,
    user: data.user,
    error: data.error
  });
}

/**
 * UserCreate — Type-safe command executor
 *
 * Usage:
 *   import { UserCreate } from '...shared/UserCreateTypes';
 *   const result = await UserCreate.execute({ ... });
 */
export const UserCreate = {
  execute(params: CommandInput<UserCreateParams>): Promise<UserCreateResult> {
    return Commands.execute<UserCreateParams, UserCreateResult>('user/create', params as Partial<UserCreateParams>);
  },
  commandName: 'user/create' as const,
} as const;
