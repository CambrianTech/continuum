/**
 * ElementPool - Component Recycling for Efficient DOM Management
 *
 * Instead of destroying and recreating DOM elements on every state change,
 * ElementPool maintains a pool of hidden elements that can be reused.
 *
 * This is the key to React-like performance in web components:
 * - hide() instead of remove() - elements go back to the pool
 * - acquire() reuses pooled elements instead of creating new ones
 * - DOM operations are minimal (display: none vs appendChild)
 *
 * @example
 * ```typescript
 * // Create pool with factory function
 * const messagePool = new ElementPool<MessageRow>(
 *   () => document.createElement('message-row') as MessageRow
 * );
 *
 * // Acquire element (reuses from pool or creates new)
 * const row = messagePool.acquire('msg-123');
 * row.message = messageData;
 * container.appendChild(row);
 *
 * // Release element back to pool (hides, doesn't destroy)
 * messagePool.release('msg-123');
 *
 * // Later, acquire will reuse the released element
 * const sameRow = messagePool.acquire('msg-456');  // Reuses hidden element
 * ```
 */

export interface ElementPoolConfig {
  /** Maximum pool size (prevents memory bloat) */
  maxPoolSize?: number;
  /** Whether to reset element state on release */
  resetOnRelease?: boolean;
  /** Custom reset function for elements */
  resetFn?: (element: HTMLElement) => void;
  /** Debug logging */
  debug?: boolean;
  /** Pool name for logging */
  name?: string;
}

export interface PoolStats {
  /** Number of elements currently in use */
  active: number;
  /** Number of elements in the pool (available for reuse) */
  pooled: number;
  /** Total elements created since pool creation */
  totalCreated: number;
  /** Number of times an element was reused from pool */
  reuses: number;
}

/**
 * Generic element pool for component recycling
 */
export class ElementPool<T extends HTMLElement> {
  /** Elements currently in use, keyed by ID */
  private active = new Map<string, T>();

  /** Available elements for reuse */
  private pool: T[] = [];

  /** Factory function to create new elements */
  private factory: () => T;

  /** Configuration */
  private config: Required<Omit<ElementPoolConfig, 'resetFn'>> & { resetFn?: (element: HTMLElement) => void };

  /** Statistics */
  private stats = {
    totalCreated: 0,
    reuses: 0
  };

  constructor(factory: () => T, config: ElementPoolConfig = {}) {
    this.factory = factory;
    this.config = {
      maxPoolSize: config.maxPoolSize ?? 50,
      resetOnRelease: config.resetOnRelease ?? true,
      resetFn: config.resetFn,
      debug: config.debug ?? false,
      name: config.name ?? 'ElementPool'
    };
  }

  /**
   * Acquire an element for use
   *
   * If an element with this ID is already active, returns it.
   * Otherwise, tries to reuse from pool or creates new.
   *
   * @param id - Unique identifier for this element
   * @returns Element ready for use
   */
  acquire(id: string): T {
    // Check if already active
    const existing = this.active.get(id);
    if (existing) {
      this.log(`Returning existing active element: ${id}`);
      return existing;
    }

    let element: T;

    // Try to reuse from pool
    if (this.pool.length > 0) {
      element = this.pool.pop()!;
      element.style.display = '';
      this.stats.reuses++;
      this.log(`Reused pooled element for: ${id} (reuse #${this.stats.reuses})`);
    } else {
      // Create new element
      element = this.factory();
      this.stats.totalCreated++;
      this.log(`Created new element for: ${id} (total: ${this.stats.totalCreated})`);
    }

    // Track as active
    this.active.set(id, element);
    element.setAttribute('data-pool-id', id);

    return element;
  }

  /**
   * Release an element back to the pool
   *
   * Element is hidden and made available for reuse.
   *
   * @param id - ID of the element to release
   * @returns true if element was released, false if not found
   */
  release(id: string): boolean {
    const element = this.active.get(id);
    if (!element) {
      this.log(`Release failed - element not found: ${id}`);
      return false;
    }

    // Remove from active tracking
    this.active.delete(id);

    // Check pool capacity
    if (this.pool.length >= this.config.maxPoolSize) {
      // Pool full - actually remove the element
      element.remove();
      this.log(`Pool full, removed element: ${id}`);
      return true;
    }

    // Reset element state
    if (this.config.resetOnRelease) {
      if (this.config.resetFn) {
        this.config.resetFn(element);
      } else {
        this.defaultReset(element);
      }
    }

    // Hide and add to pool
    element.style.display = 'none';
    element.removeAttribute('data-pool-id');
    this.pool.push(element);
    this.log(`Released to pool: ${id} (pool size: ${this.pool.length})`);

    return true;
  }

  /**
   * Get an active element by ID
   */
  get(id: string): T | undefined {
    return this.active.get(id);
  }

  /**
   * Check if an element is active
   */
  has(id: string): boolean {
    return this.active.has(id);
  }

  /**
   * Get all active element IDs
   */
  activeIds(): string[] {
    return Array.from(this.active.keys());
  }

  /**
   * Release all elements back to pool
   */
  releaseAll(): void {
    for (const id of this.active.keys()) {
      this.release(id);
    }
  }

  /**
   * Clear pool and remove all elements
   */
  clear(): void {
    // Remove all pooled elements
    for (const element of this.pool) {
      element.remove();
    }
    this.pool = [];

    // Remove all active elements
    for (const element of this.active.values()) {
      element.remove();
    }
    this.active.clear();

    this.log('Pool cleared');
  }

  /**
   * Get pool statistics
   */
  getStats(): PoolStats {
    return {
      active: this.active.size,
      pooled: this.pool.length,
      totalCreated: this.stats.totalCreated,
      reuses: this.stats.reuses
    };
  }

  /**
   * Default element reset (clears common state)
   */
  private defaultReset(element: HTMLElement): void {
    // Clear classes added during use (keep original)
    const originalClasses = element.getAttribute('data-original-classes');
    if (originalClasses !== null) {
      element.className = originalClasses;
    }

    // Clear inline styles except display
    element.removeAttribute('style');

    // Clear data attributes (except pool-id)
    const dataAttrs = Array.from(element.attributes)
      .filter(attr => attr.name.startsWith('data-') && attr.name !== 'data-pool-id');
    for (const attr of dataAttrs) {
      element.removeAttribute(attr.name);
    }
  }

  private log(...args: unknown[]): void {
    if (this.config.debug) {
      console.log(`🔄 ${this.config.name}:`, ...args);
    }
  }
}

/**
 * Convenience function to create a typed element pool
 */
export function createElementPool<T extends HTMLElement>(
  factory: () => T,
  config?: ElementPoolConfig
): ElementPool<T> {
  return new ElementPool(factory, config);
}

/**
 * Pool registry - manage multiple pools by name
 */
export class PoolRegistry {
  private pools = new Map<string, ElementPool<HTMLElement>>();

  register<T extends HTMLElement>(
    name: string,
    factory: () => T,
    config?: Omit<ElementPoolConfig, 'name'>
  ): ElementPool<T> {
    const pool = new ElementPool(factory, { ...config, name });
    this.pools.set(name, pool as ElementPool<HTMLElement>);
    return pool;
  }

  get<T extends HTMLElement>(name: string): ElementPool<T> | undefined {
    return this.pools.get(name) as ElementPool<T> | undefined;
  }

  getAllStats(): Record<string, PoolStats> {
    const stats: Record<string, PoolStats> = {};
    for (const [name, pool] of this.pools) {
      stats[name] = pool.getStats();
    }
    return stats;
  }

  clearAll(): void {
    for (const pool of this.pools.values()) {
      pool.clear();
    }
    this.pools.clear();
  }
}

/** Global pool registry */
export const poolRegistry = new PoolRegistry();
