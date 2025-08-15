/**
 * Adapter Type Guards and Utilities
 * 
 * ARCHITECTURAL PATTERN: Centralized type narrowing for adapter configurations.
 * This eliminates duplicate type checking logic and provides consistent validation
 * across all components that work with participant adapters.
 * 
 * DESIGN PRINCIPLES:
 * - Single source of truth for type validation
 * - Consistent error messages
 * - Runtime type safety with compile-time guarantees
 * - Abstract the complexity, expose simple interfaces
 */

import type {
  AdapterConfig,
  AIAdapterConfig,
  WebhookAdapterConfig,
  LoRAAdapterConfig,
  TemplateAdapterConfig,
  BrowserAdapterConfig,
  ParticipantAdapter
} from './ChatTypes';

/**
 * Type guard for AI adapter config
 */
export function isAIAdapterConfig(config: unknown): config is AIAdapterConfig {
  return (
    typeof config === 'object' && 
    config !== null &&
    'type' in config && 
    (config as any).type === 'ai-api' &&
    'provider' in config &&
    typeof (config as any).provider === 'string'
  );
}

/**
 * Type guard for webhook adapter config
 */
export function isWebhookAdapterConfig(config: unknown): config is WebhookAdapterConfig {
  return (
    typeof config === 'object' && 
    config !== null &&
    'type' in config && 
    (config as any).type === 'webhook' &&
    'url' in config &&
    typeof (config as any).url === 'string'
  );
}

/**
 * Type guard for LoRA adapter config
 */
export function isLoRAAdapterConfig(config: unknown): config is LoRAAdapterConfig {
  return (
    typeof config === 'object' && 
    config !== null &&
    'type' in config && 
    (config as any).type === 'lora-persona' &&
    'personaName' in config &&
    typeof (config as any).personaName === 'string' &&
    'modelPath' in config &&
    typeof (config as any).modelPath === 'string'
  );
}

/**
 * Type guard for template adapter config
 */
export function isTemplateAdapterConfig(config: unknown): config is TemplateAdapterConfig {
  return (
    typeof config === 'object' && 
    config !== null &&
    'type' in config && 
    (config as any).type === 'template' &&
    'template' in config &&
    typeof (config as any).template === 'string'
  );
}

/**
 * Type guard for browser adapter config
 */
export function isBrowserAdapterConfig(config: unknown): config is BrowserAdapterConfig {
  return (
    typeof config === 'object' && 
    config !== null &&
    'type' in config && 
    (config as any).type === 'browser-ui'
  );
}

/**
 * Safe adapter property extraction with type narrowing
 * 
 * ARCHITECTURAL BENEFIT: This abstraction eliminates the need for manual
 * type checking in every component that needs adapter properties.
 */
export class AdapterPropertyExtractor {
  /**
   * Safely extract AI adapter properties for legacy compatibility
   */
  static extractAIProperties(adapter: ParticipantAdapter): {
    provider?: string;
    model?: string;
    settings?: Record<string, unknown>;
  } {
    if (!adapter.config) {
      return {};
    }

    if (isAIAdapterConfig(adapter.config)) {
      return {
        provider: adapter.config.provider,
        model: adapter.config.model,
        settings: {
          apiKey: adapter.config.apiKey,
          systemPrompt: adapter.config.systemPrompt,
          maxTokens: adapter.config.maxTokens,
          temperature: adapter.config.temperature
        }
      };
    }

    // For non-AI adapters, return empty or sensible defaults
    return {};
  }

  /**
   * Get adapter configuration as generic Record for legacy compatibility
   */
  static extractGenericConfig(adapter: ParticipantAdapter): Record<string, unknown> {
    if (!adapter.config) {
      return {};
    }

    // Convert strongly typed config to generic record for legacy systems
    if (isAIAdapterConfig(adapter.config)) {
      const { type, ...rest } = adapter.config;
      return rest;
    }

    if (isWebhookAdapterConfig(adapter.config)) {
      const { type, ...rest } = adapter.config;
      return rest;
    }

    if (isLoRAAdapterConfig(adapter.config)) {
      const { type, ...rest } = adapter.config;
      return rest;
    }

    if (isTemplateAdapterConfig(adapter.config)) {
      const { type, ...rest } = adapter.config;
      return rest;
    }

    if (isBrowserAdapterConfig(adapter.config)) {
      const { type, ...rest } = adapter.config;
      return rest;
    }

    // Fallback for custom adapters
    if (typeof adapter.config === 'object' && adapter.config !== null) {
      return adapter.config as Record<string, unknown>;
    }

    return {};
  }

  /**
   * Validate adapter configuration completeness
   */
  static validateAdapterConfig(adapter: ParticipantAdapter): {
    isValid: boolean;
    errors: string[];
  } {
    const errors: string[] = [];

    if (!adapter.config) {
      errors.push('Missing adapter configuration');
      return { isValid: false, errors };
    }

    switch (adapter.type) {
      case 'ai-api':
        if (!isAIAdapterConfig(adapter.config)) {
          errors.push('Invalid AI adapter config: missing required fields');
        }
        break;
      
      case 'webhook':
        if (!isWebhookAdapterConfig(adapter.config)) {
          errors.push('Invalid webhook adapter config: missing URL');
        }
        break;
      
      case 'lora-persona':
        if (!isLoRAAdapterConfig(adapter.config)) {
          errors.push('Invalid LoRA adapter config: missing personaName or modelPath');
        }
        break;
      
      case 'template':
        if (!isTemplateAdapterConfig(adapter.config)) {
          errors.push('Invalid template adapter config: missing template');
        }
        break;
      
      case 'browser-ui':
        if (!isBrowserAdapterConfig(adapter.config)) {
          errors.push('Invalid browser adapter config');
        }
        break;
      
      default:
        // Custom adapters - basic validation
        if (typeof adapter.config !== 'object') {
          errors.push('Custom adapter config must be an object');
        }
        break;
    }

    return {
      isValid: errors.length === 0,
      errors
    };
  }
}