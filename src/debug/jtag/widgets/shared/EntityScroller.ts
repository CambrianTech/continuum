/**
 * Entity Scroller - Clean React-like + Rust-like Interface
 *
 * Elegant abstraction combining React's simplicity with Rust's type safety.
 * Clean interfaces, efficient data handling, no coupling to specific implementations.
 */

import type { BaseEntity } from '../../system/data/entities/BaseEntity';
import { EntityManager } from './EntityManager';

// Pure render function - like React components, constrained to our BaseEntity
export type RenderFn<T extends BaseEntity> = (entity: T, context: RenderContext<T>) => HTMLElement;

// Context passed to render functions
export interface RenderContext<T extends BaseEntity> {
  readonly index: number;
  readonly total: number;
  readonly isCurrentUser?: boolean;
  readonly customData?: Record<string, unknown>;
}

// Data loader - clean async interface
export type LoadFn<T extends BaseEntity> = (cursor?: string, limit?: number) => Promise<LoadResult<T>>;

// Load result - standard pagination result
export interface LoadResult<T extends BaseEntity> {
  readonly items: readonly T[];
  readonly hasMore: boolean;
  readonly nextCursor?: string;
}

// Configuration - minimal but complete
export interface ScrollerConfig {
  readonly pageSize: number;
  readonly direction: 'newest-first' | 'oldest-first';
  readonly threshold?: number;
  readonly rootMargin?: string;
  readonly autoScroll?: {
    readonly enabled: boolean;
    readonly threshold: number; // Distance from natural end before auto-scroll triggers
    readonly behavior: 'smooth' | 'instant';
  };
}

// Clean scroller interface - like React hooks
export interface EntityScroller<T extends BaseEntity> {
  // Core operations
  readonly load: () => Promise<void>;
  readonly loadMore: () => Promise<void>;
  readonly refresh: () => Promise<void>;

  // Real-time updates
  readonly add: (entity: T, position?: 'start' | 'end') => void;
  readonly update: (id: string, entity: T) => boolean;
  readonly remove: (id: string) => boolean;

  // Smart real-time updates with auto-scroll
  readonly addWithAutoScroll: (entity: T, position?: 'start' | 'end') => void;

  // State queries
  readonly entities: () => readonly T[];
  readonly loading: () => boolean;
  readonly hasMore: () => boolean;

  // Cleanup
  readonly destroy: () => void;
}

/**
 * Create Entity Scroller - One clean function
 */
export function createScroller<T extends BaseEntity>(
  container: HTMLElement,
  render: RenderFn<T>,
  load: LoadFn<T>,
  config: ScrollerConfig,
  context?: Omit<RenderContext<T>, 'index' | 'total'>
): EntityScroller<T> {

  // Internal state with EntityManager for robust entity management
  const entityManager = new EntityManager<T>({
    name: `Scroller-${config.direction}`,
    maxSize: config.pageSize * 10, // Reasonable limit to prevent memory issues
    debugMode: true
  });
  let isLoading = false;
  let hasMoreItems = true;
  let cursor: string | undefined;
  let observer: IntersectionObserver | undefined;
  let sentinel: HTMLElement | undefined;

  // Smart scroll utilities - check if user is near bottom (where new content appears)
  const isNearEnd = (threshold: number = config.autoScroll?.threshold || 100): boolean => {
    const { scrollTop, scrollHeight, clientHeight } = container;

    // Always check distance from bottom since new items are added at 'end' (bottom)
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
    return distanceFromBottom <= threshold;
  };

  const scrollToEnd = (behavior: 'smooth' | 'instant' = config.autoScroll?.behavior || 'smooth'): void => {
    const scrollOptions: ScrollToOptions = {
      behavior: behavior === 'smooth' ? 'smooth' : 'auto',
      // Always scroll to bottom since new items are always added at 'end' (bottom)
      top: container.scrollHeight
    };

    container.scrollTo(scrollOptions);
  };

  const smartScrollToNewContent = (): void => {
    if (!config.autoScroll?.enabled) return;

    if (isNearEnd()) {
      console.log(`📍 EntityScroller: User near new content area, auto-scrolling`);
      scrollToEnd();
    } else {
      console.log(`📍 EntityScroller: User reading away from new content area, not auto-scrolling`);
    }
  };

  // Efficient DOM operations using fragments - only add genuinely new entities
  const addEntitiesToDOM = (newEntities: readonly T[], position: 'start' | 'end'): void => {
    const fragment = document.createDocumentFragment();
    const validEntities: T[] = [];

    // First check which entities are actually new
    newEntities.forEach((entity) => {
      if (entityManager.add(entity)) {
        validEntities.push(entity);
      }
    });

    // Only render genuinely new entities
    validEntities.forEach((entity, idx) => {
      const renderContext: RenderContext<T> = {
        ...context,
        index: idx,
        total: entityManager.count()
      };

      const element = render(entity, renderContext);
      element.setAttribute('data-entity-id', entity.id || (entity as any).messageId || 'unknown');
      fragment.appendChild(element);
    });

    // Always append to DOM - CSS handles display direction
    if (validEntities.length > 0) {
      container.appendChild(fragment);
    }
  };

  // Intersection observer for infinite scroll
  const setupObserver = (): void => {
    observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        if (entry?.isIntersecting && hasMoreItems && !isLoading) {
          scroller.loadMore();
        }
      },
      {
        root: container,
        // Use standard positive rootMargin for all scroll directions
        rootMargin: config.rootMargin || '50px',
        threshold: config.threshold || 0.1
      }
    );

    // Create sentinel
    sentinel = document.createElement('div');
    sentinel.className = 'entity-scroller-sentinel';
    sentinel.style.cssText = 'height: 1px; opacity: 0; pointer-events: none; margin: 0; padding: 0; border: 0; position: absolute; top: -1px;';
    console.log(`🔧 CLAUDE-FIX-${Date.now()}: EntityScroller sentinel created with improved positioning for ${config.direction}`);

    // For newest-first (chat), sentinel at TOP triggers when scrolling UP to load older messages
    if (config.direction === 'newest-first') {
      container.insertBefore(sentinel, container.firstChild);
    } else {
      container.appendChild(sentinel);
    }

    observer.observe(sentinel);
  };

  // The clean API object
  const scroller: EntityScroller<T> = {
    // Load initial data
    async load(): Promise<void> {
      if (isLoading) return;

      isLoading = true;
      try {
        const result = await load(undefined, config.pageSize);

        entityManager.clear();
        container.innerHTML = '';

        if (result.items.length > 0) {
          addEntitiesToDOM(result.items, 'end');
          hasMoreItems = result.hasMore;
          cursor = result.nextCursor;

          // Setup observer AFTER DOM is painted
          requestAnimationFrame(() => {
            setupObserver();
          });
        } else {
          setupObserver();
        }
      } catch (error) {
        console.error('❌ EntityScroller: Error during load():', error);
        hasMoreItems = false;
      } finally {
        isLoading = false;
      }
    },

    // Load more data
    async loadMore(): Promise<void> {
      if (isLoading || !hasMoreItems) {
        return;
      }

      isLoading = true;
      try {
        const result = await load(cursor, config.pageSize);

        if (result.items.length > 0) {
          // Always append - CSS handles visual direction
          addEntitiesToDOM(result.items, 'end');
          hasMoreItems = result.hasMore;
          cursor = result.nextCursor;

          // Keep sentinel at bottom for intersection observer
          if (sentinel) {
            container.appendChild(sentinel);
          }
        } else {
          hasMoreItems = false;
        }
      } catch (error) {
        console.error('❌ EntityScroller: Error in loadMore():', error);
      } finally {
        isLoading = false;
      }
    },

    // Refresh all data
    async refresh(): Promise<void> {
      cursor = undefined;
      hasMoreItems = true;
      await scroller.load();
    },

    // Real-time updates with automatic deduplication and replacement
    add(entity: T, position = 'end'): void {
      const entityId = entity.id || (entity as any).messageId || 'unknown';

      // Check for duplicate using EntityManager
      if (entityManager.has(entityId)) {
        console.warn(`⚠️ EntityScroller: Duplicate entity blocked: ${entityId}`);
        // Update existing entity
        entityManager.update(entityId, entity);

        // Update DOM element if it exists
        const existingElement = container.querySelector(`[data-entity-id="${entityId}"]`);
        if (existingElement) {
          const renderContext: RenderContext<T> = { index: 0, total: entityManager.count() };
          const newElement = render(entity, renderContext);
          newElement.setAttribute('data-entity-id', entityId);
          existingElement.replaceWith(newElement);
        }

        console.log(`🔧 EntityScroller: Updated existing entity with ID: ${entityId}`);
        return;
      }

      // SECOND: Check DOM for orphaned elements (shouldn't happen but defensive)
      const existingElement = container.querySelector(`[data-entity-id="${entityId}"]`);
      if (existingElement) {
        console.warn(`⚠️ EntityScroller: Found orphaned DOM element for ID: ${entityId}, removing it`);
        existingElement.remove();
      }

      // Add entity - addEntitiesToDOM handles deduplication via EntityManager
      addEntitiesToDOM([entity], 'end');
      console.log(`🔧 CLAUDE-SCROLLER-DEBUG: EntityScroller.add() processed entity with ID: ${entityId}, total entities now: ${entityManager.count()}`);
    },

    // Smart real-time updates with intrinsic direction awareness and replacement
    addWithAutoScroll(entity: T, position?: 'start' | 'end'): void {
      const entityId = entity.id || (entity as any).messageId || 'unknown';

      // Check for duplicate using EntityManager
      if (entityManager.has(entityId)) {
        console.warn(`⚠️ EntityScroller: Duplicate entity blocked: ${entityId}`);
        // Update existing entity - no auto-scroll for updates
        entityManager.update(entityId, entity);

        // Update DOM element if it exists
        const existingElement = container.querySelector(`[data-entity-id="${entityId}"]`);
        if (existingElement) {
          const renderContext: RenderContext<T> = { index: 0, total: entityManager.count() };
          const newElement = render(entity, renderContext);
          newElement.setAttribute('data-entity-id', entityId);
          existingElement.replaceWith(newElement);
        }

        console.log(`🔧 EntityScroller: Updated existing entity with ID: ${entityId}`);
        return;
      }

      // SECOND: Check DOM for orphaned elements (shouldn't happen but defensive)
      const existingElement = container.querySelector(`[data-entity-id="${entityId}"]`);
      if (existingElement) {
        console.warn(`⚠️ EntityScroller: Found orphaned DOM element for ID: ${entityId}, removing it`);
        existingElement.remove();
      }

      // Determine where "new content" naturally goes based on direction
      const newContentPosition = config.direction === 'newest-first' ? 'end' : 'start';
      const actualPosition = position || newContentPosition;

      // Track count before attempting add
      const initialCount = entityManager.count();

      // Add entity - addEntitiesToDOM handles deduplication via EntityManager
      addEntitiesToDOM([entity], 'end');
      console.log(`🔧 CLAUDE-SCROLLER-DEBUG: EntityScroller.addWithAutoScroll() processed entity with ID: ${entityId}, total entities now: ${entityManager.count()}`);

      // Auto-scroll only if entity was genuinely added
      if (entityManager.count() > initialCount) {
        requestAnimationFrame(() => {
          smartScrollToNewContent();
        });
      }
    },

    update(id: string, entity: T): boolean {
      if (!entityManager.update(id, entity)) {
        return false;
      }

      const element = container.querySelector(`[data-entity-id="${id}"]`) as HTMLElement;
      if (element) {
        const renderContext: RenderContext<T> = {
          ...context,
          index: 0, // Index not critical for display since CSS handles layout
          total: entityManager.count()
        };

        const newElement = render(entity, renderContext);
        newElement.setAttribute('data-entity-id', entity.id);
        element.replaceWith(newElement);
      }

      return true;
    },

    remove(id: string): boolean {
      if (!entityManager.remove(id)) {
        return false;
      }

      container.querySelector(`[data-entity-id="${id}"]`)?.remove();
      return true;
    },

    // State queries - clean getters
    entities: () => entityManager.getAll(),
    loading: () => isLoading,
    hasMore: () => hasMoreItems,

    // Cleanup
    destroy(): void {
      observer?.disconnect();
      sentinel?.remove();
      entityManager.clear();
    }
  };

  return scroller;
}

// Convenient presets
export const SCROLLER_PRESETS = {
  CHAT: {
    pageSize: 20,
    direction: 'newest-first' as const,
    threshold: 0.1,
    rootMargin: '50px',
    autoScroll: {
      enabled: true,
      threshold: 100, // 100px from bottom
      behavior: 'smooth' as const
    }
  },

  LIST: {
    pageSize: 50,
    direction: 'oldest-first' as const,
    threshold: 0.2,
    rootMargin: '100px',
    autoScroll: {
      enabled: false, // Lists typically don't need auto-scroll
      threshold: 100,
      behavior: 'smooth' as const
    }
  }
} as const;