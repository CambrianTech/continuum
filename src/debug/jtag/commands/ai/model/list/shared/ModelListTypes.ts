/**
 * Model List Command - Types
 *
 * Lists available AI models with their capabilities and metadata
 * Like AVCaptureDevice.DiscoverySession - enumerate available devices
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

/**
 * Model capability constraints (like camera position, resolution)
 */
export interface ModelCapabilities {
  // Size constraints
  maxParameters?: string;           // e.g., "3B", "7B", "13B"
  minParameters?: string;

  // Performance constraints
  maxLatency?: number;               // Max acceptable latency in ms
  minTokensPerSecond?: number;       // Min throughput

  // Feature requirements
  supportsJSON?: boolean;            // Structured output
  supportsToolCalling?: boolean;     // Function calling
  supportsFunctionCalling?: boolean; // Same as tool calling
  supportsStreaming?: boolean;       // Streaming responses

  // Context requirements
  minContextLength?: number;         // Minimum context window (tokens)
  maxContextLength?: number;         // Maximum context window

  // Task type hints
  taskType?: 'reasoning' | 'creative' | 'classification' | 'extraction' | 'code' | 'chat' | 'gating';

  // Provider constraints
  preferredProviders?: string[];     // e.g., ["candle", "openai"]
  excludeProviders?: string[];
}

/**
 * Model metadata (what we know about each model)
 */
export interface ModelInfo {
  // Identity
  name: string;                      // e.g., "llama3.2:1b"
  displayName: string;               // e.g., "Llama 3.2 1B"
  provider: string;                  // e.g., "candle", "openai"

  // Capabilities
  parameters: string;                // e.g., "1B", "3B", "70B"
  contextLength: number;             // Context window size
  supportsJSON: boolean;
  supportsToolCalling: boolean;
  supportsStreaming: boolean;

  // Performance characteristics
  estimatedLatency: number;          // Average latency in ms
  estimatedTokensPerSecond: number;  // Throughput

  // Resource requirements
  memoryRequirement: string;         // e.g., "1.3GB", "2.0GB"
  recommendedFor: string[];          // e.g., ["gating", "classification"]

  // Availability
  available: boolean;                // Is it currently available?
  needsDownload?: boolean;           // Needs to be pulled first
}

/**
 * Model list params
 */
export interface ModelListParams extends CommandParams {
  // Optional filtering
  capabilities?: ModelCapabilities;
  providerFilter?: string;           // Filter by specific provider
  includeUnavailable?: boolean;      // Include models that need download
}

/**
 * Model list result
 */
export interface ModelListResult extends CommandResult {
  success: boolean;
  models: ModelInfo[];
  totalCount: number;
  availableCount: number;
  providers: string[];               // List of providers with models
  error?: string;
}

/**
 * ModelList — Type-safe command executor
 *
 * Usage:
 *   import { ModelList } from '...shared/ModelListTypes';
 *   const result = await ModelList.execute({ ... });
 */
export const ModelList = {
  execute(params: CommandInput<ModelListParams>): Promise<ModelListResult> {
    return Commands.execute<ModelListParams, ModelListResult>('ai/model/list', params as Partial<ModelListParams>);
  },
  commandName: 'ai/model/list' as const,
} as const;
