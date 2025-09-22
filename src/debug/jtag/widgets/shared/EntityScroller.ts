/**
 * Entity Scroller - Clean React-like + Rust-like Interface
 *
 * Elegant abstraction combining React's simplicity with Rust's type safety.
 * Clean interfaces, efficient data handling, no coupling to specific implementations.
 */

import type { BaseEntity } from '../../system/data/domains/CoreTypes';

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

  // Internal state
  let entities: T[] = [];
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

  // Efficient DOM operations using fragments
  const addEntitiesToDOM = (newEntities: readonly T[], position: 'start' | 'end'): void => {
    const fragment = document.createDocumentFragment();

    newEntities.forEach((entity, idx) => {
      const renderContext: RenderContext<T> = {
        ...context,
        index: position === 'start' ? idx : entities.length + idx,
        total: entities.length + newEntities.length
      };

      const element = render(entity, renderContext);
      element.setAttribute('data-entity-id', entity.id || (entity as any).messageId || 'unknown');
      fragment.appendChild(element);
    });

    // Batch DOM update - efficient
    if (position === 'start') {
      container.insertBefore(fragment, container.firstChild);
      entities = [...newEntities, ...entities];
    } else {
      container.appendChild(fragment);
      entities = [...entities, ...newEntities];
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

        entities = [];
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
          const position = config.direction === 'newest-first' ? 'start' : 'end';
          addEntitiesToDOM(result.items, position);
          hasMoreItems = result.hasMore;
          cursor = result.nextCursor;

          // Reposition sentinel after adding items
          if (sentinel) {
            if (config.direction === 'newest-first' && position === 'start') {
              // For newest-first (chat), keep sentinel at top when adding older messages to start
              container.insertBefore(sentinel, container.firstChild);
            } else if (config.direction === 'oldest-first' && position === 'end') {
              // For oldest-first (lists), keep sentinel at bottom when adding newer items to end
              container.appendChild(sentinel);
            }
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
      // Check if entity already exists in DOM using data-entity-id
      const entityId = entity.id || (entity as any).messageId || 'unknown';
      const existingElement = container.querySelector(`[data-entity-id="${entityId}"]`);
      if (existingElement) {
        // Replace existing entity with updated one
        const entityIndex = entities.findIndex(e => e.id === entity.id);
        if (entityIndex !== -1) {
          entities[entityIndex] = entity; // Update in entities array

          // Re-render the element with updated data
          const renderContext: RenderContext<T> = {
            index: entityIndex,
            total: entities.length
          };
          const newElement = render(entity, renderContext);
          newElement.setAttribute('data-entity-id', entityId);
          existingElement.replaceWith(newElement);

          console.log(`🔧 EntityScroller: Replaced existing entity with ID: ${entity.id}`);
          return;
        }
      }

      // Add new entity to entities array and DOM
      entities.push(entity);
      addEntitiesToDOM([entity], position);
      console.log(`🔧 EntityScroller: Added new entity with ID: ${entity.id}`);
    },

    // Smart real-time updates with intrinsic direction awareness and replacement
    addWithAutoScroll(entity: T, position?: 'start' | 'end'): void {
      // Check if entity already exists in DOM using data-entity-id
      const entityId = entity.id || (entity as any).messageId || 'unknown';
      const existingElement = container.querySelector(`[data-entity-id="${entityId}"]`);
      if (existingElement) {
        // Replace existing entity with updated one
        const entityIndex = entities.findIndex(e => e.id === entity.id);
        if (entityIndex !== -1) {
          entities[entityIndex] = entity; // Update in entities array

          // Re-render the element with updated data
          const renderContext: RenderContext<T> = {
            index: entityIndex,
            total: entities.length
          };
          const newElement = render(entity, renderContext);
          newElement.setAttribute('data-entity-id', entityId);
          existingElement.replaceWith(newElement);

          console.log(`🔧 EntityScroller: Replaced existing entity with ID: ${entity.id}`);
          return; // Don't auto-scroll for replacements
        }
      }

      // Determine where "new content" naturally goes based on direction
      const newContentPosition = config.direction === 'newest-first' ? 'end' : 'start';
      const actualPosition = position || newContentPosition;

      // Add new entity to entities array and DOM
      entities.push(entity);
      addEntitiesToDOM([entity], actualPosition);

      // Auto-scroll only if this is truly new content being added to the natural position
      if (actualPosition === newContentPosition) {
        requestAnimationFrame(() => {
          smartScrollToNewContent();
        });
      }
    },

    update(id: string, entity: T): boolean {
      const index = entities.findIndex(e => e.id === id);
      if (index === -1) return false;

      entities[index] = entity;

      const element = container.querySelector(`[data-entity-id="${id}"]`) as HTMLElement;
      if (element) {
        const renderContext: RenderContext<T> = {
          ...context,
          index,
          total: entities.length
        };

        const newElement = render(entity, renderContext);
        newElement.setAttribute('data-entity-id', entity.id);
        element.replaceWith(newElement);
      }

      return true;
    },

    remove(id: string): boolean {
      const index = entities.findIndex(e => e.id === id);
      if (index === -1) return false;

      entities.splice(index, 1);
      container.querySelector(`[data-entity-id="${id}"]`)?.remove();

      return true;
    },

    // State queries - clean getters
    entities: () => entities,
    loading: () => isLoading,
    hasMore: () => hasMoreItems,

    // Cleanup
    destroy(): void {
      observer?.disconnect();
      sentinel?.remove();
      entities = [];
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