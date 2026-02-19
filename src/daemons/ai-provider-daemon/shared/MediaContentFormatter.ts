/**
 * MediaContentFormatter - Provider-Agnostic Multimodal Content Formatting
 * =========================================================================
 *
 * Unified interface for formatting multimodal content (images, audio, video)
 * into provider-specific API formats.
 *
 * Architecture:
 * - Single source of truth for content formatting
 * - Each provider has its own formatting strategy
 * - Handles both text extraction (for non-vision models) and full multimodal
 *
 * Supported Providers:
 * - OpenAI-compatible (OpenAI, Together, Fireworks, Groq, DeepSeek, XAI)
 * - Anthropic (proprietary format)
 * - Candle (local inference with base64 images)
 *
 * Why centralized?
 * - Eliminates duplicate formatMultimodalContent() implementations
 * - Single place to add new providers
 * - Consistent image handling (base64, URL, size limits)
 * - Enables eventual Rust worker architecture
 */

import type { ContentPart, ImageInput } from './AIProviderTypesV2';

/**
 * Provider-specific content format types
 */

// OpenAI-compatible format
export interface OpenAIContentPart {
  type: 'text' | 'image_url';
  text?: string;
  image_url?: {
    url: string;
    detail?: 'auto' | 'low' | 'high';
  };
}

// Anthropic format
export interface AnthropicContentPart {
  type: 'text' | 'image';
  text?: string;
  source?: {
    type: 'base64' | 'url';
    media_type?: string;
    data?: string;
    url?: string;
  };
}

// Local inference chat format (images at message level)
export interface LocalChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
  images?: string[];  // Base64-encoded images (no data URL prefix)
}

/**
 * MediaContentFormatter - Provider-specific formatting strategies
 */
export class MediaContentFormatter {
  /**
   * Format multimodal content for OpenAI-compatible APIs
   * (OpenAI, Together, Fireworks, Groq, XAI, DeepSeek)
   *
   * OpenAI format:
   * { type: 'text', text: '...' }
   * { type: 'image_url', image_url: { url: 'data:image/png;base64,...' } }
   */
  static formatForOpenAI(content: ContentPart[], detail: 'auto' | 'low' | 'high' = 'auto'): OpenAIContentPart[] {
    return content
      .filter(part => part.type !== 'tool_use' && part.type !== 'tool_result') // Tool blocks handled by adapter
      .map(part => {
        if (part.type === 'text') {
          return { type: 'text', text: part.text };
        }

        if (part.type === 'image') {
          const imageUrl = this.getImageUrl(part.image);
          return {
            type: 'image_url',
            image_url: {
              url: imageUrl,
              detail,
            },
          };
        }

        // Audio/video: extract as text placeholder for now
        return { type: 'text', text: `[${part.type}]` };
      });
  }

  /**
   * Format multimodal content for Anthropic API
   *
   * Anthropic format:
   * { type: 'text', text: '...' }
   * { type: 'image', source: { type: 'base64', media_type: 'image/png', data: '...' } }
   */
  static formatForAnthropic(content: ContentPart[]): AnthropicContentPart[] {
    return content
      .filter(part => part.type !== 'tool_use' && part.type !== 'tool_result') // Tool blocks handled by adapter
      .map(part => {
        if (part.type === 'text') {
          return { type: 'text', text: part.text };
        }

        if (part.type === 'image') {
          const image = part.image;

          // Prefer base64 for Anthropic
          if (image.base64) {
            return {
              type: 'image',
              source: {
                type: 'base64',
                media_type: image.mimeType || 'image/png',
                data: image.base64,
              },
            };
          }

          // URL fallback
          if (image.url) {
            return {
              type: 'image',
              source: {
                type: 'url',
                url: image.url,
              },
            };
          }
        }

        // Audio/video: extract as text placeholder
        return { type: 'text', text: `[${part.type}]` };
      });
  }

  /**
   * Format messages for local inference Chat API
   *
   * Local format - images are at message level, not inline:
   * { role: 'user', content: 'Describe this image', images: ['base64...'] }
   *
   * @returns Local chat message with content and images array
   */
  static formatForLocal(
    role: 'system' | 'user' | 'assistant',
    content: string | ContentPart[]
  ): LocalChatMessage {
    // Simple string content
    if (typeof content === 'string') {
      return { role, content };
    }

    // Multimodal content - extract text and images separately
    const textParts: string[] = [];
    const images: string[] = [];

    for (const part of content) {
      if (part.type === 'text') {
        textParts.push(part.text);
      } else if (part.type === 'image') {
        const base64 = this.extractBase64(part.image);
        if (base64) {
          images.push(base64);
        }
      }
    }

    const message: LocalChatMessage = {
      role,
      content: textParts.join('\n'),
    };

    if (images.length > 0) {
      message.images = images;
    }

    return message;
  }

  /**
   * Extract text-only content (for non-vision models)
   * Strips all images and returns concatenated text
   */
  static extractTextOnly(content: ContentPart[]): string {
    return content
      .filter(part => part.type === 'text' || part.type === 'tool_result')
      .map(part => {
        if (part.type === 'text') return part.text;
        if (part.type === 'tool_result') return part.content;
        return '';
      })
      .join('\n');
  }

  /**
   * Check if content contains any images
   */
  static hasImages(content: string | ContentPart[]): boolean {
    if (typeof content === 'string') {
      return false;
    }
    return content.some(part => part.type === 'image');
  }

  /**
   * Count images in content
   */
  static countImages(content: string | ContentPart[]): number {
    if (typeof content === 'string') {
      return 0;
    }
    return content.filter(part => part.type === 'image').length;
  }

  /**
   * Get image URL from ImageInput (handles both URL and base64)
   */
  private static getImageUrl(image: ImageInput): string {
    if (image.url) {
      return image.url;
    }

    if (image.base64) {
      const mimeType = image.mimeType || 'image/png';
      return `data:${mimeType};base64,${image.base64}`;
    }

    return '';
  }

  /**
   * Extract raw base64 data from ImageInput (no data URL prefix)
   * Used by local inference which expects raw base64
   */
  private static extractBase64(image: ImageInput): string | null {
    if (image.base64) {
      // Remove data URL prefix if present
      if (image.base64.startsWith('data:')) {
        const commaIndex = image.base64.indexOf(',');
        return image.base64.substring(commaIndex + 1);
      }
      return image.base64;
    }

    // TODO: For URLs, could fetch and convert to base64
    // For now, skip URL-based images for local inference
    return null;
  }

  /**
   * Validate image against provider constraints
   */
  static validateImage(
    image: ImageInput,
    constraints: {
      maxSize?: number;        // Max dimension in pixels
      supportedFormats?: string[];
      maxBase64Length?: number;
    }
  ): { valid: boolean; error?: string } {
    // Check format
    if (constraints.supportedFormats && image.mimeType) {
      const format = image.mimeType.split('/')[1];
      if (!constraints.supportedFormats.includes(format)) {
        return {
          valid: false,
          error: `Unsupported image format: ${format}. Supported: ${constraints.supportedFormats.join(', ')}`,
        };
      }
    }

    // Check base64 size (rough check - actual image dimensions require decoding)
    if (constraints.maxBase64Length && image.base64) {
      if (image.base64.length > constraints.maxBase64Length) {
        return {
          valid: false,
          error: `Image too large: ${image.base64.length} chars (max: ${constraints.maxBase64Length})`,
        };
      }
    }

    return { valid: true };
  }

  /**
   * Estimate base64 size from image dimensions
   * Useful for pre-flight checks before encoding
   */
  static estimateBase64Size(width: number, height: number, format: 'png' | 'jpeg' = 'png'): number {
    // PNG: ~2-4 bytes per pixel (compressed)
    // JPEG: ~0.5-1 bytes per pixel (compressed)
    const bytesPerPixel = format === 'png' ? 3 : 0.75;
    const rawBytes = width * height * bytesPerPixel;

    // Base64 increases size by ~33%
    return Math.ceil(rawBytes * 1.33);
  }
}

/**
 * Export convenience function for common use case
 */
export function formatContentForProvider(
  provider: string,
  content: ContentPart[]
): OpenAIContentPart[] | AnthropicContentPart[] {
  switch (provider) {
    case 'anthropic':
      return MediaContentFormatter.formatForAnthropic(content);
    case 'openai':
    case 'together':
    case 'fireworks':
    case 'groq':
    case 'xai':
    case 'deepseek':
      return MediaContentFormatter.formatForOpenAI(content);
    default:
      // Default to OpenAI format (most common)
      return MediaContentFormatter.formatForOpenAI(content);
  }
}
