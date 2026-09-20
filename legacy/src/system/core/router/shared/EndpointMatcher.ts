/**
 * Endpoint Matcher - Hierarchical Endpoint Resolution
 * 
 * Handles hierarchical endpoint matching for JTAG message routing.
 * Supports exact matches and parent path fallbacks.
 * 
 * CORE FUNCTIONALITY:
 * - Exact endpoint matching for direct routes
 * - Hierarchical fallback for nested endpoints (commands/screenshot -> commands)
 * - Efficient lookup with caching for performance
 * - Flexible matching strategies
 * 
 * USAGE EXAMPLES:
 * - "server/commands/screenshot" matches "server/commands" registration
 * - "browser/health/ping" matches "browser/health" registration
 * - "console/error" matches exact "console/error" registration
 */

import type { MessageSubscriber } from './JTAGRouter';

export interface EndpointMatchResult<T = MessageSubscriber> {
  subscriber: T;
  matchedEndpoint: string;
  originalEndpoint: string;
  matchType: 'exact' | 'hierarchical';
}

export interface EndpointMatcherConfig {
  enableHierarchicalMatching: boolean;
  enableCaching: boolean;
  maxCacheSize: number;
}

export class EndpointMatcher<T = MessageSubscriber> {
  private subscribers = new Map<string, T>();
  private matchCache = new Map<string, EndpointMatchResult<T>>();
  private config: EndpointMatcherConfig;

  constructor(config: Partial<EndpointMatcherConfig> = {}) {
    this.config = {
      enableHierarchicalMatching: true,
      enableCaching: true,
      maxCacheSize: 1000,
      ...config
    };
  }

  /**
   * Register a subscriber for an endpoint
   */
  register(endpoint: string, subscriber: T): void {
    this.subscribers.set(endpoint, subscriber);
    // Clear cache when registration changes
    if (this.config.enableCaching) {
      this.clearCache();
    }
  }

  /**
   * Unregister a subscriber from an endpoint
   */
  unregister(endpoint: string): boolean {
    const removed = this.subscribers.delete(endpoint);
    if (removed && this.config.enableCaching) {
      this.clearCache();
    }
    return removed;
  }

  /**
   * Find subscriber for an endpoint with hierarchical matching
   */
  match(targetEndpoint: string): EndpointMatchResult<T> | null {
    // Check cache first
    if (this.config.enableCaching) {
      const cached = this.matchCache.get(targetEndpoint);
      if (cached) {
        return cached;
      }
    }

    const result = this.performMatch(targetEndpoint);
    
    // Cache the result
    if (result && this.config.enableCaching) {
      this.cacheResult(targetEndpoint, result);
    }

    return result;
  }

  /**
   * Get subscriber directly (exact match only)
   */
  getExact(endpoint: string): T | undefined {
    return this.subscribers.get(endpoint);
  }

  /**
   * Check if endpoint has exact registration
   */
  hasExact(endpoint: string): boolean {
    return this.subscribers.has(endpoint);
  }

  /**
   * Get all registered endpoints
   */
  getEndpoints(): string[] {
    return Array.from(this.subscribers.keys());
  }

  /**
   * Get subscriber count
   */
  size(): number {
    return this.subscribers.size;
  }

  /**
   * Clear all registrations
   */
  clear(): void {
    this.subscribers.clear();
    this.clearCache();
  }

  /**
   * Perform the actual matching logic
   */
  private performMatch(targetEndpoint: string): EndpointMatchResult<T> | null {
    // Try exact match first
    const exactSubscriber = this.subscribers.get(targetEndpoint);
    if (exactSubscriber) {
      return {
        subscriber: exactSubscriber,
        matchedEndpoint: targetEndpoint,
        originalEndpoint: targetEndpoint,
        matchType: 'exact'
      };
    }

    // If hierarchical matching is disabled, stop here
    if (!this.config.enableHierarchicalMatching) {
      return null;
    }

    // Try hierarchical matching
    const endpointParts = targetEndpoint.split('/');
    
    // Try progressively shorter paths: "server/commands/screenshot" -> "server/commands" -> "commands"
    for (let i = endpointParts.length - 1; i > 0; i--) {
      const parentEndpoint = endpointParts.slice(0, i).join('/');
      const parentSubscriber = this.subscribers.get(parentEndpoint);
      
      if (parentSubscriber) {
        return {
          subscriber: parentSubscriber,
          matchedEndpoint: parentEndpoint,
          originalEndpoint: targetEndpoint,
          matchType: 'hierarchical'
        };
      }
    }

    return null;
  }

  /**
   * Cache a match result
   */
  private cacheResult(endpoint: string, result: EndpointMatchResult<T>): void {
    // Implement LRU cache eviction if needed
    if (this.matchCache.size >= this.config.maxCacheSize) {
      const firstKey = this.matchCache.keys().next().value;
      if (firstKey) {
        this.matchCache.delete(firstKey);
      }
    }

    this.matchCache.set(endpoint, result);
  }

  /**
   * Clear match cache
   */
  private clearCache(): void {
    this.matchCache.clear();
  }

  /**
   * Get cache statistics
   */
  getCacheStats(): { size: number; hitRate: number } {
    // This would need hit/miss counters for accurate hit rate
    return {
      size: this.matchCache.size,
      hitRate: 0 // Placeholder - would need proper tracking
    };
  }
}