/**
 * AICapabilityRegistry - Unified AI Capability Discovery
 * =======================================================
 *
 * Central registry for all AI capabilities across providers.
 * Enables AIs to discover what they and others can do.
 *
 * Architecture:
 * - Single source of truth for AI capabilities
 * - Providers register their capabilities on initialization
 * - AIs can query for specific capabilities
 * - Supports capability delegation (AI A routes to AI B for vision)
 *
 * Capability Categories:
 * - Input: vision (image-input), audio-input, video-input
 * - Output: text, audio-output (TTS), image-generation, video-generation
 * - Processing: embeddings, function-calling, tool-use
 *
 * Why centralized?
 * - AIs can discover their own and others' capabilities
 * - Enables intelligent routing (text-only AI → vision AI for images)
 * - Single place for capability management
 * - Supports eventual multi-AI collaboration architecture
 */

import { VisionCapabilityService, type VisionModelEntry } from './VisionCapabilityService';

/**
 * All supported capability types
 */
export type AICapability =
  // Input capabilities
  | 'text-input'         // Can process text (all LLMs)
  | 'image-input'        // Can process images (vision models)
  | 'audio-input'        // Can process audio (STT, audio understanding)
  | 'video-input'        // Can process video

  // Output capabilities
  | 'text-output'        // Can generate text (all LLMs)
  | 'image-output'       // Can generate images (DALL-E, SD)
  | 'audio-output'       // Can generate audio (TTS)
  | 'video-output'       // Can generate video

  // Processing capabilities
  | 'embeddings'         // Can generate embeddings
  | 'function-calling'   // Native function/tool calling
  | 'streaming'          // Supports streaming responses
  | 'multimodal'         // Can handle mixed modalities
  | 'context-window-large' // 100k+ context
  | 'context-window-huge'  // 200k+ context
  | 'reasoning'          // Strong reasoning (Opus, o1, etc.)
  | 'coding'             // Optimized for code
  | 'math';              // Optimized for math

/**
 * Provider capability registration
 */
export interface ProviderCapabilities {
  providerId: string;
  providerName: string;
  models: ModelCapabilityInfo[];
  defaultCapabilities: AICapability[];  // Capabilities all models share
}

/**
 * Per-model capability info
 */
export interface ModelCapabilityInfo {
  modelId: string;
  displayName: string;
  capabilities: AICapability[];
  contextWindow?: number;
  maxOutputTokens?: number;
  costTier?: 'free' | 'low' | 'medium' | 'high' | 'premium';
  latencyTier?: 'fast' | 'medium' | 'slow';
}

/**
 * Query result for capability searches
 */
export interface CapabilityMatch {
  providerId: string;
  modelId: string;
  displayName: string;
  capabilities: AICapability[];
  score: number;  // How well it matches (0-1)
}

/**
 * AICapabilityRegistry - Singleton
 *
 * Usage:
 *   const registry = AICapabilityRegistry.getInstance();
 *
 *   // Check if model has capability
 *   if (registry.hasCapability('openai', 'gpt-4o', 'image-input')) {
 *     // Use vision features
 *   }
 *
 *   // Find all models with a capability
 *   const visionModels = registry.findModelsWithCapability('image-input');
 *
 *   // Find best model for a set of capabilities
 *   const bestModel = registry.findBestMatch(['image-input', 'reasoning']);
 */
export class AICapabilityRegistry {
  private static instance: AICapabilityRegistry | null = null;

  // Registry: providerId -> ProviderCapabilities
  private providers: Map<string, ProviderCapabilities> = new Map();

  // Cache for capability queries
  private queryCache: Map<string, CapabilityMatch[]> = new Map();

  // Vision service integration
  private visionService: VisionCapabilityService;

  private constructor() {
    this.visionService = VisionCapabilityService.getInstance();
    this.initializeBuiltInProviders();
  }

  static getInstance(): AICapabilityRegistry {
    if (!AICapabilityRegistry.instance) {
      AICapabilityRegistry.instance = new AICapabilityRegistry();
    }
    return AICapabilityRegistry.instance;
  }

  /**
   * Register a provider's capabilities
   */
  registerProvider(capabilities: ProviderCapabilities): void {
    this.providers.set(capabilities.providerId, capabilities);
    this.clearCache();
  }

  /**
   * Check if a specific model has a capability
   */
  hasCapability(providerId: string, modelId: string, capability: AICapability): boolean {
    // Special handling for vision - delegate to VisionCapabilityService
    if (capability === 'image-input') {
      return this.visionService.supportsVision(providerId, modelId);
    }

    const provider = this.providers.get(providerId);
    if (!provider) return false;

    // Check default capabilities
    if (provider.defaultCapabilities.includes(capability)) {
      return true;
    }

    // Check model-specific capabilities
    const model = provider.models.find(m =>
      m.modelId === modelId || modelId.includes(m.modelId)
    );
    return model?.capabilities.includes(capability) ?? false;
  }

  /**
   * Get all capabilities for a specific model
   */
  getCapabilities(providerId: string, modelId: string): AICapability[] {
    const capabilities = new Set<AICapability>();

    // Add vision if supported
    if (this.visionService.supportsVision(providerId, modelId)) {
      capabilities.add('image-input');
    }

    const provider = this.providers.get(providerId);
    if (provider) {
      // Add default capabilities
      for (const cap of provider.defaultCapabilities) {
        capabilities.add(cap);
      }

      // Add model-specific capabilities
      const model = provider.models.find(m =>
        m.modelId === modelId || modelId.includes(m.modelId)
      );
      if (model) {
        for (const cap of model.capabilities) {
          capabilities.add(cap);
        }
      }
    }

    return Array.from(capabilities);
  }

  /**
   * Find all models with a specific capability
   */
  findModelsWithCapability(capability: AICapability): CapabilityMatch[] {
    const cacheKey = `single:${capability}`;
    if (this.queryCache.has(cacheKey)) {
      return this.queryCache.get(cacheKey)!;
    }

    const matches: CapabilityMatch[] = [];

    for (const [providerId, provider] of this.providers) {
      for (const model of provider.models) {
        if (this.hasCapability(providerId, model.modelId, capability)) {
          matches.push({
            providerId,
            modelId: model.modelId,
            displayName: model.displayName,
            capabilities: this.getCapabilities(providerId, model.modelId),
            score: 1.0,
          });
        }
      }
    }

    this.queryCache.set(cacheKey, matches);
    return matches;
  }

  /**
   * Find best model matching multiple capabilities
   * Returns models sorted by match score (how many capabilities they have)
   */
  findBestMatch(requiredCapabilities: AICapability[], preferredCapabilities?: AICapability[]): CapabilityMatch[] {
    const cacheKey = `multi:${requiredCapabilities.join(',')}:${preferredCapabilities?.join(',') || ''}`;
    if (this.queryCache.has(cacheKey)) {
      return this.queryCache.get(cacheKey)!;
    }

    const matches: CapabilityMatch[] = [];

    for (const [providerId, provider] of this.providers) {
      for (const model of provider.models) {
        // Check all required capabilities
        const hasAllRequired = requiredCapabilities.every(cap =>
          this.hasCapability(providerId, model.modelId, cap)
        );

        if (!hasAllRequired) continue;

        // Calculate score based on preferred capabilities
        let score = 1.0;
        if (preferredCapabilities && preferredCapabilities.length > 0) {
          const preferredMatches = preferredCapabilities.filter(cap =>
            this.hasCapability(providerId, model.modelId, cap)
          ).length;
          score = preferredMatches / preferredCapabilities.length;
        }

        matches.push({
          providerId,
          modelId: model.modelId,
          displayName: model.displayName,
          capabilities: this.getCapabilities(providerId, model.modelId),
          score,
        });
      }
    }

    // Sort by score descending
    matches.sort((a, b) => b.score - a.score);

    this.queryCache.set(cacheKey, matches);
    return matches;
  }

  /**
   * Get all registered providers
   */
  getProviders(): string[] {
    return Array.from(this.providers.keys());
  }

  /**
   * Get capability info for a provider
   */
  getProviderInfo(providerId: string): ProviderCapabilities | undefined {
    return this.providers.get(providerId);
  }

  /**
   * Clear query cache (called when providers are registered)
   */
  private clearCache(): void {
    this.queryCache.clear();
  }

  /**
   * Initialize built-in provider capabilities
   */
  private initializeBuiltInProviders(): void {
    // ================================
    // Anthropic
    // ================================
    this.registerProvider({
      providerId: 'anthropic',
      providerName: 'Anthropic',
      defaultCapabilities: [
        'text-input', 'text-output', 'image-input', 'function-calling',
        'streaming', 'multimodal', 'context-window-huge', 'reasoning', 'coding'
      ],
      models: [
        {
          modelId: 'claude-3-5-sonnet',
          displayName: 'Claude 3.5 Sonnet',
          capabilities: ['reasoning', 'coding', 'math'],
          contextWindow: 200000,
          costTier: 'medium',
          latencyTier: 'medium',
        },
        {
          modelId: 'claude-3-opus',
          displayName: 'Claude 3 Opus',
          capabilities: ['reasoning', 'coding', 'math'],
          contextWindow: 200000,
          costTier: 'premium',
          latencyTier: 'slow',
        },
        {
          modelId: 'claude-3-haiku',
          displayName: 'Claude 3 Haiku',
          capabilities: ['coding'],
          contextWindow: 200000,
          costTier: 'low',
          latencyTier: 'fast',
        },
      ],
    });

    // ================================
    // OpenAI
    // ================================
    this.registerProvider({
      providerId: 'openai',
      providerName: 'OpenAI',
      defaultCapabilities: [
        'text-input', 'text-output', 'function-calling', 'streaming'
      ],
      models: [
        {
          modelId: 'gpt-4o',
          displayName: 'GPT-4o',
          capabilities: ['image-input', 'multimodal', 'reasoning', 'coding', 'context-window-large'],
          contextWindow: 128000,
          costTier: 'medium',
          latencyTier: 'fast',
        },
        {
          modelId: 'gpt-4-turbo',
          displayName: 'GPT-4 Turbo',
          capabilities: ['image-input', 'multimodal', 'reasoning', 'coding', 'context-window-large'],
          contextWindow: 128000,
          costTier: 'medium',
          latencyTier: 'medium',
        },
        {
          modelId: 'gpt-4-vision',
          displayName: 'GPT-4 Vision',
          capabilities: ['image-input', 'multimodal', 'reasoning'],
          contextWindow: 128000,
          costTier: 'high',
          latencyTier: 'slow',
        },
        {
          modelId: 'gpt-3.5-turbo',
          displayName: 'GPT-3.5 Turbo',
          capabilities: ['coding'],
          contextWindow: 16385,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'dall-e-3',
          displayName: 'DALL-E 3',
          capabilities: ['image-output'],
          costTier: 'medium',
          latencyTier: 'slow',
        },
        {
          modelId: 'whisper',
          displayName: 'Whisper',
          capabilities: ['audio-input'],
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'tts-1',
          displayName: 'TTS-1',
          capabilities: ['audio-output'],
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'text-embedding',
          displayName: 'Text Embedding',
          capabilities: ['embeddings'],
          costTier: 'low',
          latencyTier: 'fast',
        },
      ],
    });

    // ================================
    // Ollama (Local)
    // Model names must match EXACTLY what `ollama list` shows
    // ================================
    this.registerProvider({
      providerId: 'ollama',
      providerName: 'Ollama',
      defaultCapabilities: ['text-input', 'text-output', 'embeddings'],
      models: [
        {
          modelId: 'llama3.2:3b',
          displayName: 'Llama 3.2 3B',
          capabilities: ['coding', 'reasoning'],
          contextWindow: 128000,
          costTier: 'free',
          latencyTier: 'fast',
        },
        {
          modelId: 'llama3.2:1b',
          displayName: 'Llama 3.2 1B',
          capabilities: ['coding'],
          contextWindow: 128000,
          costTier: 'free',
          latencyTier: 'fast',
        },
        {
          modelId: 'llava:7b',
          displayName: 'LLaVA 7B',
          capabilities: ['image-input', 'multimodal'],
          contextWindow: 4096,
          costTier: 'free',
          latencyTier: 'medium',
        },
        {
          modelId: 'phi3:mini',
          displayName: 'Phi-3 Mini',
          capabilities: ['coding', 'reasoning'],
          contextWindow: 128000,
          costTier: 'free',
          latencyTier: 'fast',
        },
        {
          modelId: 'nomic-embed-text:latest',
          displayName: 'Nomic Embed',
          capabilities: ['embeddings'],
          costTier: 'free',
          latencyTier: 'fast',
        },
        {
          modelId: 'all-minilm:latest',
          displayName: 'All-MiniLM',
          capabilities: ['embeddings'],
          costTier: 'free',
          latencyTier: 'fast',
        },
      ],
    });

    // ================================
    // Together AI
    // ================================
    this.registerProvider({
      providerId: 'together',
      providerName: 'Together AI',
      defaultCapabilities: ['text-input', 'text-output', 'streaming'],
      models: [
        {
          modelId: 'meta-llama/Llama-3.2-90B-Vision-Instruct-Turbo',
          displayName: 'Llama 3.2 90B Vision',
          capabilities: ['image-input', 'multimodal', 'reasoning', 'context-window-large'],
          contextWindow: 128000,
          costTier: 'medium',
          latencyTier: 'medium',
        },
        {
          modelId: 'meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo',
          displayName: 'Llama 3.1 405B',
          capabilities: ['reasoning', 'coding', 'context-window-large'],
          contextWindow: 128000,
          costTier: 'high',
          latencyTier: 'slow',
        },
      ],
    });

    // ================================
    // XAI (Grok)
    // ================================
    this.registerProvider({
      providerId: 'xai',
      providerName: 'xAI',
      defaultCapabilities: ['text-input', 'text-output', 'streaming'],
      models: [
        {
          modelId: 'grok-2-vision',
          displayName: 'Grok 2 Vision',
          capabilities: ['image-input', 'multimodal', 'reasoning'],
          contextWindow: 32768,
          costTier: 'medium',
          latencyTier: 'fast',
        },
        {
          modelId: 'grok-2',
          displayName: 'Grok 2',
          capabilities: ['reasoning', 'coding'],
          contextWindow: 32768,
          costTier: 'medium',
          latencyTier: 'fast',
        },
      ],
    });

    // ================================
    // Groq
    // ================================
    this.registerProvider({
      providerId: 'groq',
      providerName: 'Groq',
      defaultCapabilities: ['text-input', 'text-output', 'streaming'],
      models: [
        {
          modelId: 'llama-3.2-90b-vision-preview',
          displayName: 'Llama 3.2 90B Vision',
          capabilities: ['image-input', 'multimodal', 'reasoning'],
          contextWindow: 8192,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'llama-3.1-70b-versatile',
          displayName: 'Llama 3.1 70B',
          capabilities: ['reasoning', 'coding'],
          contextWindow: 32768,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'mixtral-8x7b-32768',
          displayName: 'Mixtral 8x7B',
          capabilities: ['coding'],
          contextWindow: 32768,
          costTier: 'low',
          latencyTier: 'fast',
        },
      ],
    });
  }

  /**
   * Debug: Print all registered capabilities
   */
  debugPrint(): void {
    console.log('=== AICapabilityRegistry ===');
    for (const [providerId, provider] of this.providers) {
      console.log(`\n${provider.providerName} (${providerId}):`);
      console.log(`  Default: ${provider.defaultCapabilities.join(', ')}`);
      for (const model of provider.models) {
        console.log(`  - ${model.displayName} (${model.modelId})`);
        console.log(`    Capabilities: ${model.capabilities.join(', ')}`);
        if (model.contextWindow) {
          console.log(`    Context: ${model.contextWindow.toLocaleString()}`);
        }
      }
    }
  }
}

// Export singleton accessor
export const capabilityRegistry = AICapabilityRegistry.getInstance;
