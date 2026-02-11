/**
 * ModelProvider - Flexible AI model selection for Sentinels
 *
 * Sentinels can be:
 * 1. Script-only (no AI) - BuildSentinel, VisualSentinel
 * 2. AI-powered - OrchestratorSentinel, future PlannerSentinel
 *
 * For AI-powered sentinels, select models by:
 * - Power level (capacity enum)
 * - Specific model string
 * - Provider preference
 */

import { execSync } from 'child_process';

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
  LOCAL = 'local',         // Local inference service (Qwen, etc.)
  OLLAMA = 'ollama',       // Ollama models
  ANTHROPIC = 'anthropic', // Claude API
  OPENAI = 'openai',       // OpenAI API
  AUTO = 'auto',           // Auto-select best available
}

/**
 * Model selection config
 */
export interface ModelConfig {
  provider?: ModelProvider;
  capacity?: ModelCapacity;
  model?: string;          // Specific model override (e.g., 'claude-3-opus', 'llama3.2:3b')
  maxTokens?: number;
  temperature?: number;
}

/**
 * Known models by provider and capacity
 */
const MODEL_REGISTRY: Record<ModelProvider, Partial<Record<ModelCapacity, string>>> = {
  [ModelProvider.LOCAL]: {
    [ModelCapacity.TINY]: 'qwen2.5:0.5b',
    [ModelCapacity.SMALL]: 'qwen2.5:1.5b',
    [ModelCapacity.MEDIUM]: 'qwen2.5:7b',
    [ModelCapacity.LARGE]: 'qwen2.5:32b',
    [ModelCapacity.SOTA]: 'qwen2.5:72b',
  },
  [ModelProvider.OLLAMA]: {
    [ModelCapacity.TINY]: 'phi3:mini',
    [ModelCapacity.SMALL]: 'llama3.2:3b',
    [ModelCapacity.MEDIUM]: 'llama3.1:8b',
    [ModelCapacity.LARGE]: 'llama3.1:70b',
    [ModelCapacity.SOTA]: 'llama3.1:405b',
  },
  [ModelProvider.ANTHROPIC]: {
    [ModelCapacity.SMALL]: 'claude-3-haiku-20240307',
    [ModelCapacity.MEDIUM]: 'claude-3-5-sonnet-20241022',
    [ModelCapacity.LARGE]: 'claude-3-5-sonnet-20241022',
    [ModelCapacity.SOTA]: 'claude-3-opus-20240229',
  },
  [ModelProvider.OPENAI]: {
    [ModelCapacity.SMALL]: 'gpt-4o-mini',
    [ModelCapacity.MEDIUM]: 'gpt-4o',
    [ModelCapacity.LARGE]: 'gpt-4o',
    [ModelCapacity.SOTA]: 'gpt-4o',
  },
  [ModelProvider.AUTO]: {},  // Determined at runtime
};

/**
 * Model inference result
 */
export interface InferenceResult {
  success: boolean;
  text?: string;
  error?: string;
  model: string;
  provider: ModelProvider;
  tokensUsed?: number;
}

/**
 * ModelSelector - resolves config to actual model
 */
export class ModelSelector {
  private workingDir: string;

  constructor(workingDir: string) {
    this.workingDir = workingDir;
  }

  /**
   * Resolve model config to specific model string
   */
  resolve(config: ModelConfig): { provider: ModelProvider; model: string } {
    // If specific model given, use it
    if (config.model) {
      return {
        provider: config.provider || ModelProvider.AUTO,
        model: config.model,
      };
    }

    // Resolve by capacity
    const capacity = config.capacity || ModelCapacity.SMALL;
    const provider = config.provider || ModelProvider.LOCAL;

    if (provider === ModelProvider.AUTO) {
      return this.autoSelect(capacity);
    }

    const model = MODEL_REGISTRY[provider][capacity];
    if (!model) {
      // Fallback to closest available
      const available = Object.entries(MODEL_REGISTRY[provider]);
      if (available.length > 0) {
        return { provider, model: available[0][1] as string };
      }
      throw new Error(`No models available for provider ${provider}`);
    }

    return { provider, model };
  }

  /**
   * Auto-select best available model for capacity
   */
  private autoSelect(capacity: ModelCapacity): { provider: ModelProvider; model: string } {
    // Priority: LOCAL > OLLAMA > ANTHROPIC > OPENAI
    // (prefer local/free over API)

    // Check if local inference is available
    try {
      execSync('./jtag ping', { cwd: this.workingDir, stdio: 'pipe' });
      const model = MODEL_REGISTRY[ModelProvider.LOCAL][capacity];
      if (model) {
        return { provider: ModelProvider.LOCAL, model };
      }
    } catch {
      // Local not available
    }

    // Check Ollama
    try {
      execSync('ollama list', { stdio: 'pipe' });
      const model = MODEL_REGISTRY[ModelProvider.OLLAMA][capacity];
      if (model) {
        return { provider: ModelProvider.OLLAMA, model };
      }
    } catch {
      // Ollama not available
    }

    // Fallback to Anthropic if API key exists
    if (process.env.ANTHROPIC_API_KEY) {
      const model = MODEL_REGISTRY[ModelProvider.ANTHROPIC][capacity];
      if (model) {
        return { provider: ModelProvider.ANTHROPIC, model };
      }
    }

    // Last resort: OpenAI
    if (process.env.OPENAI_API_KEY) {
      const model = MODEL_REGISTRY[ModelProvider.OPENAI][capacity];
      if (model) {
        return { provider: ModelProvider.OPENAI, model };
      }
    }

    throw new Error('No AI providers available');
  }

  /**
   * List available models for a provider
   */
  listModels(provider: ModelProvider): string[] {
    return Object.values(MODEL_REGISTRY[provider]).filter(Boolean) as string[];
  }
}

/**
 * ModelInvoker - actually calls the model
 */
export class ModelInvoker {
  private workingDir: string;
  private selector: ModelSelector;

  constructor(workingDir: string) {
    this.workingDir = workingDir;
    this.selector = new ModelSelector(workingDir);
  }

  /**
   * Generate text from a prompt
   */
  async generate(prompt: string, config: ModelConfig = {}): Promise<InferenceResult> {
    const { provider, model } = this.selector.resolve(config);
    const maxTokens = config.maxTokens || 2000;

    switch (provider) {
      case ModelProvider.LOCAL:
        return this.invokeLocal(prompt, model, maxTokens);
      case ModelProvider.OLLAMA:
        return this.invokeOllama(prompt, model, maxTokens);
      case ModelProvider.ANTHROPIC:
        return this.invokeAnthropic(prompt, model, maxTokens);
      case ModelProvider.OPENAI:
        return this.invokeOpenAI(prompt, model, maxTokens);
      case ModelProvider.AUTO:
        // Auto already resolved to specific provider
        return this.invokeLocal(prompt, model, maxTokens);
      default:
        return { success: false, error: `Unknown provider: ${provider}`, model, provider };
    }
  }

  private async invokeLocal(prompt: string, model: string, maxTokens: number): Promise<InferenceResult> {
    try {
      const escapedPrompt = prompt.replace(/"/g, '\\"').replace(/\n/g, '\\n');
      const response = execSync(
        `./jtag inference/generate --prompt="${escapedPrompt}" --maxTokens=${maxTokens}`,
        { cwd: this.workingDir, encoding: 'utf-8', timeout: 120000 }
      );
      const parsed = JSON.parse(response);
      if (parsed.success && parsed.text) {
        return { success: true, text: parsed.text, model, provider: ModelProvider.LOCAL };
      }
      return { success: false, error: parsed.error || 'No response', model, provider: ModelProvider.LOCAL };
    } catch (error: any) {
      return { success: false, error: error.message, model, provider: ModelProvider.LOCAL };
    }
  }

  private async invokeOllama(prompt: string, model: string, maxTokens: number): Promise<InferenceResult> {
    try {
      const escapedPrompt = prompt.replace(/'/g, "'\\''");
      const response = execSync(
        `ollama run ${model} '${escapedPrompt}' 2>/dev/null`,
        { encoding: 'utf-8', timeout: 120000, maxBuffer: 10 * 1024 * 1024 }
      );
      return { success: true, text: response.trim(), model, provider: ModelProvider.OLLAMA };
    } catch (error: any) {
      return { success: false, error: error.message, model, provider: ModelProvider.OLLAMA };
    }
  }

  private async invokeAnthropic(prompt: string, model: string, maxTokens: number): Promise<InferenceResult> {
    // Use JTAG's ai/generate which handles Anthropic
    try {
      const escapedPrompt = prompt.replace(/"/g, '\\"').replace(/\n/g, '\\n');
      const response = execSync(
        `./jtag ai/generate --prompt="${escapedPrompt}" --model="${model}" --maxTokens=${maxTokens}`,
        { cwd: this.workingDir, encoding: 'utf-8', timeout: 120000 }
      );
      const parsed = JSON.parse(response);
      if (parsed.success && parsed.text) {
        return { success: true, text: parsed.text, model, provider: ModelProvider.ANTHROPIC };
      }
      return { success: false, error: parsed.error || 'No response', model, provider: ModelProvider.ANTHROPIC };
    } catch (error: any) {
      return { success: false, error: error.message, model, provider: ModelProvider.ANTHROPIC };
    }
  }

  private async invokeOpenAI(prompt: string, model: string, maxTokens: number): Promise<InferenceResult> {
    // Similar to Anthropic, use JTAG's ai/generate
    try {
      const escapedPrompt = prompt.replace(/"/g, '\\"').replace(/\n/g, '\\n');
      const response = execSync(
        `./jtag ai/generate --prompt="${escapedPrompt}" --model="${model}" --maxTokens=${maxTokens}`,
        { cwd: this.workingDir, encoding: 'utf-8', timeout: 120000 }
      );
      const parsed = JSON.parse(response);
      if (parsed.success && parsed.text) {
        return { success: true, text: parsed.text, model, provider: ModelProvider.OPENAI };
      }
      return { success: false, error: parsed.error || 'No response', model, provider: ModelProvider.OPENAI };
    } catch (error: any) {
      return { success: false, error: error.message, model, provider: ModelProvider.OPENAI };
    }
  }
}

// Convenience functions
export function createInvoker(workingDir: string): ModelInvoker {
  return new ModelInvoker(workingDir);
}

export function resolveModel(workingDir: string, config: ModelConfig): { provider: ModelProvider; model: string } {
  return new ModelSelector(workingDir).resolve(config);
}
