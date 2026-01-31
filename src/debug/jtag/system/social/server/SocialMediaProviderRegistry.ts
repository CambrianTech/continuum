/**
 * SocialMediaProviderRegistry - Factory for creating platform provider instances
 *
 * Follows the same registry pattern as AdapterProviderRegistry.
 * Each persona gets their own provider instance (per-persona rate limiting).
 *
 * Usage:
 *   const provider = SocialMediaProviderRegistry.createProvider('moltbook');
 *   provider.authenticate(apiKey);
 *   await provider.createPost({ title: '...', content: '...', community: 'general' });
 */

import type { ISocialMediaProvider } from '../shared/ISocialMediaProvider';
import { MoltbookProvider } from './providers/MoltbookProvider';

type ProviderFactory = () => ISocialMediaProvider;

export class SocialMediaProviderRegistry {
  private static readonly factories = new Map<string, ProviderFactory>();

  static {
    // Register built-in providers
    SocialMediaProviderRegistry.register('moltbook', () => new MoltbookProvider());
  }

  /**
   * Register a new platform provider factory.
   * Call this to add support for additional social media platforms.
   */
  static register(platformId: string, factory: ProviderFactory): void {
    SocialMediaProviderRegistry.factories.set(platformId, factory);
  }

  /**
   * Create a new provider instance for a platform.
   * Each call returns a FRESH instance (per-persona rate tracking).
   */
  static createProvider(platformId: string): ISocialMediaProvider {
    const factory = SocialMediaProviderRegistry.factories.get(platformId);
    if (!factory) {
      const available = Array.from(SocialMediaProviderRegistry.factories.keys()).join(', ');
      throw new Error(`Unknown social media platform: '${platformId}'. Available: ${available}`);
    }
    return factory();
  }

  /**
   * List all registered platform IDs.
   */
  static get availablePlatforms(): string[] {
    return Array.from(SocialMediaProviderRegistry.factories.keys());
  }

  /**
   * Check if a platform is registered.
   */
  static hasPlatform(platformId: string): boolean {
    return SocialMediaProviderRegistry.factories.has(platformId);
  }
}
