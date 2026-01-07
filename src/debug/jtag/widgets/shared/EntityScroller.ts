/* eslint-disable no-undef, @typescript-eslint/no-unused-vars, @typescript-eslint/prefer-nullish-coalescing */
/**
 * Entity Scroller - Clean React-like + Rust-like Interface
 *
 * Elegant abstraction combining React's simplicity with Rust's type safety.
 * Clean interfaces, efficient data handling, no coupling to specific implementations.
 *
 * NOTE: Linting disabled for:
 * - no-undef: Browser globals (document, requestAnimationFrame) available in widget context
 * - no-unused-vars: Generic type parameter T is used in type definitions
 * - prefer-nullish-coalescing: Requires strictNullChecks not enabled project-wide
 */

import { EntityManager } from './EntityManager';
import { BaseEntity } from '../../system/data/entities/BaseEntity';

// Pure render function - works with BaseEntity for proper typing
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
  // Always appends to DOM end - CSS column-reverse handles visual positioning
  readonly add: (entity: T) => void;
  readonly update: (id: string, entity: T) => boolean;
  readonly remove: (id: string) => boolean;
  readonly clear: () => void;

  // Smart real-time updates with auto-scroll
  readonly addWithAutoScroll: (entity: T) => void;

  // Scroll control
  readonly scrollToEnd: (behavior?: 'smooth' | 'instant') => void;

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
    debugMode: false // Disable to reduce console spam
  });
  let isLoading = false;
  let hasMoreItems = true;
  let cursor: string | undefined;
  let observer: IntersectionObserver | undefined;
  let sentinel: HTMLElement | undefined;
  let observerActive = false; // Track whether observer should be running
  let idleTimeout: ReturnType<typeof setTimeout> | undefined;

  // Smart scroll utilities - check if user is near newest content
  const isNearEnd = (threshold: number = config.autoScroll?.threshold || 100): boolean => {
    const { scrollTop, scrollHeight, clientHeight } = container;

    // Discord/Slack pattern: newest at bottom, check distance from bottom
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
    return distanceFromBottom <= threshold;
  };

  const scrollToEnd = (behavior: 'smooth' | 'instant' = config.autoScroll?.behavior || 'smooth'): void => {
    // Discord/Slack pattern: scroll to actual bottom (newest messages)
    const scrollOptions: ScrollToOptions = {
      behavior: behavior === 'smooth' ? 'smooth' : 'auto',
      top: container.scrollHeight
    };

    container.scrollTo(scrollOptions);
  };

  const smartScrollToNewContent = (): void => {
    if (!config.autoScroll?.enabled) return;

    if (isNearEnd()) {
      scrollToEnd();
    }
  };

  // Handle container resize - always scroll to bottom on resize
  let resizeObserver: ResizeObserver | undefined;

  // Setup ResizeObserver for container (works inside shadow DOM)
  // ONLY for chat widgets with autoScroll enabled
  if (config.autoScroll?.enabled) {
    resizeObserver = new ResizeObserver(() => {
      // Use larger threshold for resize events (new messages growing the container)
      // This prevents stopping auto-scroll when error messages cause slight scrolls
      const threshold = 200; // More forgiving than the 10px user scroll threshold
      if (isNearEnd(threshold)) {
        scrollToEnd('instant');
      }
    });
    resizeObserver.observe(container);
  }

  // Insert single entity in correct timestamp order
  const insertEntityInOrder = (entity: T): void => {
    // Add to entity manager
    if (!entityManager.add(entity)) {
      return; // Already exists
    }

    // Render the new element
    const renderContext: RenderContext<T> = {
      ...context,
      index: 0,
      total: entityManager.count()
    };
    const newElement = render(entity, renderContext);
    const entityId = entity.id ?? (entity as Record<string, unknown>).messageId as string ?? 'unknown';
    newElement.setAttribute('data-entity-id', entityId);

    // Get timestamp for ordering (if entity has timestamp field)
    // Handle both Date objects and number timestamps
    const entityTimestampRaw = (entity as Record<string, unknown>).timestamp;
    const entityTimestamp = entityTimestampRaw instanceof Date
      ? entityTimestampRaw.getTime()
      : (typeof entityTimestampRaw === 'number' ? entityTimestampRaw : undefined);

    if (!entityTimestamp) {
      // No timestamp, just append (fallback behavior)
      container.appendChild(newElement);
      return;
    }

    // Find correct insertion point based on timestamp
    // DOM order: oldest -> newest (column-reverse CSS flips visual display)
    const children = Array.from(container.children);
    let insertBefore: Element | null = null;

    for (const child of children) {
      const childId = child.getAttribute('data-entity-id');
      if (!childId) continue;

      // Get child entity from manager
      const childEntity = entityManager.get(childId);
      if (!childEntity) continue;

      // Handle both Date objects and number timestamps
      const childTimestampRaw = (childEntity as Record<string, unknown>).timestamp;
      const childTimestamp = childTimestampRaw instanceof Date
        ? childTimestampRaw.getTime()
        : (typeof childTimestampRaw === 'number' ? childTimestampRaw : undefined);

      if (!childTimestamp) continue;

      // If new message is older than this child, insert before it
      if (entityTimestamp < childTimestamp) {
        insertBefore = child;
        break;
      }
    }

    if (insertBefore) {
      container.insertBefore(newElement, insertBefore);
      console.log(`📍 EntityScroller: Inserted message ${entityId} before ${insertBefore.getAttribute('data-entity-id')} (timestamp order)`);
    } else {
      // Newest message, append to end
      container.appendChild(newElement);
    }
  };

  // Efficient DOM operations using fragments - only add genuinely new entities
  // For newest-first: initial load appends, loadMore prepends (older messages at top)
  // For oldest-first: always appends
  const addEntitiesToDOM = (newEntities: readonly T[], prepend: boolean = false): void => {
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
      element.setAttribute('data-entity-id', entity.id ?? (entity as Record<string, unknown>).messageId as string ?? 'unknown');
      fragment.appendChild(element);
    });

    // Insert position depends on direction and whether loading more
    if (validEntities.length > 0) {
      if (prepend && config.direction === 'newest-first') {
        // Insert older messages after sentinel (at visual top for newest-first)
        const sentinel = container.querySelector('.entity-scroller-sentinel');
        if (sentinel && sentinel.nextSibling) {
          container.insertBefore(fragment, sentinel.nextSibling);
        } else {
          container.insertBefore(fragment, container.firstChild);
        }
      } else {
        // Initial load or oldest-first: append to end
        container.appendChild(fragment);
      }
    }
  };

  // Activate observer ONLY when needed (lazy + event-driven)
  const activateObserver = (): void => {
    if (!hasMoreItems || observerActive) return;

    // Calculate rootMargin as 20% of container height for smooth loading before reaching top
    const rootMarginPx = Math.max(100, container.clientHeight * 0.2);
    const rootMarginStr = `${rootMarginPx}px`;

    observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        if (entry?.isIntersecting && hasMoreItems && !isLoading) {
          console.log(`🔄 INTERSECTION: Triggering loadMore()`);
          scroller.loadMore();
        }
      },
      {
        root: container,
        rootMargin: config.rootMargin ?? rootMarginStr,
        threshold: config.threshold ?? 0.1
      }
    );

    // Create sentinel if it doesn't exist
    if (!sentinel) {
      sentinel = document.createElement('div');
      sentinel.className = 'entity-scroller-sentinel';
      sentinel.style.cssText = 'height: 1px; opacity: 0; pointer-events: none; margin: 0; padding: 0; border: 0;';

      if (config.direction === 'newest-first') {
        container.insertBefore(sentinel, container.firstChild);
      } else {
        container.appendChild(sentinel);
      }
    }

    observer.observe(sentinel);
    observerActive = true;
  };

  // Deactivate observer when idle (go silent)
  const deactivateObserver = (): void => {
    if (!observerActive) return;

    observer?.disconnect();
    observer = undefined;
    observerActive = false;
  };

  // Event-driven observer activation: activate on scroll, deactivate after idle
  const IDLE_TIMEOUT_MS = 2000; // Go idle after 2 seconds of no scroll

  const onUserScroll = (): void => {
    // Clear any pending idle timeout
    if (idleTimeout) {
      clearTimeout(idleTimeout);
    }

    // Activate observer when user scrolls (ONLY if there's more data)
    if (hasMoreItems && !observerActive) {
      activateObserver();
    }

    // Schedule deactivation after idle period
    idleTimeout = setTimeout(() => {
      deactivateObserver();
    }, IDLE_TIMEOUT_MS);
  };

  // Listen for scroll events ONLY if there's potentially more data to load
  if (hasMoreItems) {
    container.addEventListener('scroll', onUserScroll, { passive: true });
  }

  // Diff-based DOM update - only add/remove what changed
  const diffAndUpdateDOM = (newItems: readonly T[]): void => {
    const newItemsById = new Map<string, T>();
    for (const item of newItems) {
      const id = item.id ?? (item as Record<string, unknown>).messageId as string ?? 'unknown';
      newItemsById.set(id, item);
    }

    // Get existing entity IDs in DOM
    const existingIds = new Set(entityManager.getAll().map(e =>
      e.id ?? (e as Record<string, unknown>).messageId as string ?? 'unknown'
    ));
    const newIds = new Set(newItemsById.keys());

    // Remove entities no longer present (without clearing everything)
    for (const id of existingIds) {
      if (!newIds.has(id)) {
        entityManager.remove(id);
        const element = container.querySelector(`[data-entity-id="${id}"]`);
        if (element) {
          element.remove();
        }
      }
    }

    // Add new entities (addEntitiesToDOM handles duplicates)
    const itemsToAdd: T[] = [];
    for (const item of newItems) {
      const id = item.id ?? (item as Record<string, unknown>).messageId as string ?? 'unknown';
      if (!existingIds.has(id)) {
        itemsToAdd.push(item);
      }
    }

    if (itemsToAdd.length > 0) {
      addEntitiesToDOM(itemsToAdd);
    }
  };

  // The clean API object
  const scroller: EntityScroller<T> = {
    // Load initial data - uses diffing to preserve existing elements
    async load(): Promise<void> {
      if (isLoading) return;

      isLoading = true;
      try {
        const result = await load(undefined, config.pageSize);

        if (result.items.length > 0) {
          // For Discord/Slack pattern: DB returns DESC (newest→oldest)
          // Reverse to display oldest→newest in normal DOM order
          const itemsToAdd = config.direction === 'newest-first'
            ? [...result.items].reverse()
            : result.items;

          // Use diff-based update instead of clearing everything
          diffAndUpdateDOM(itemsToAdd);

          hasMoreItems = result.hasMore;
          cursor = result.nextCursor;

          // For newest-first (chat), scroll to bottom to show latest messages
          if (config.direction === 'newest-first') {
            scrollToEnd('instant');
          }
        } else {
          // No items - clear if we had items before
          if (entityManager.count() > 0) {
            entityManager.clear();
            // Remove all entity elements but keep sentinel
            const entityElements = container.querySelectorAll('[data-entity-id]');
            entityElements.forEach(el => el.remove());
          }
          hasMoreItems = false;
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
          // For Discord/Slack pattern: DB returns DESC (newest→oldest)
          // Reverse to display oldest→newest in normal DOM order
          const itemsToAdd = config.direction === 'newest-first'
            ? [...result.items].reverse()
            : result.items;

          // When loading more, prepend for newest-first (older messages go at top)
          addEntitiesToDOM(itemsToAdd, true);
          hasMoreItems = result.hasMore;
          cursor = result.nextCursor;

          // Discord/Slack pattern: Keep sentinel at top for loading older messages
          if (sentinel && config.direction === 'newest-first') {
            container.insertBefore(sentinel, container.firstChild);
          } else if (sentinel) {
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
    // Always appends to DOM end - CSS column-reverse handles visual positioning
    add(entity: T): void {
      const entityId = entity.id ?? (entity as Record<string, unknown>).messageId as string ?? 'unknown';

      // Check for duplicate using EntityManager
      if (entityManager.has(entityId)) {
        console.log(`🔄 EntityScroller: Updating existing entity: ${entityId}`);
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

      // Add entity - insert in correct timestamp order for messages
      insertEntityInOrder(entity);
    },

    // Smart real-time updates with auto-scroll
    // Always appends to DOM end - CSS column-reverse handles visual positioning
    addWithAutoScroll(entity: T): void {
      const entityId = entity.id ?? (entity as Record<string, unknown>).messageId as string ?? 'unknown';

      // Check for duplicate using EntityManager
      if (entityManager.has(entityId)) {
        console.log(`🔄 EntityScroller: Updating existing entity: ${entityId}`);
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

      // Track count before attempting add
      const initialCount = entityManager.count();

      // CHECK SCROLL POSITION BEFORE ADDING (critical: must check before DOM changes)
      const wasAtBottom = config.autoScroll?.enabled && isNearEnd();

      // Add entity - insert in correct timestamp order
      insertEntityInOrder(entity);

      // Auto-scroll only if entity was genuinely added AND user was at bottom
      if (entityManager.count() > initialCount && wasAtBottom) {
        // Scroll directly - DOM is already updated synchronously
        scrollToEnd();
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

    clear(): void {
      entityManager.clear();
      // Remove entity elements but preserve sentinel and structure
      const entityElements = container.querySelectorAll('[data-entity-id]');
      entityElements.forEach(el => el.remove());
      hasMoreItems = true;
      cursor = undefined;
    },

    // Scroll control - exposed publicly
    scrollToEnd(behavior: 'smooth' | 'instant' = 'smooth'): void {
      scrollToEnd(behavior);
    },

    // State queries - clean getters
    entities: () => entityManager.getAll(),
    loading: () => isLoading,
    hasMore: () => hasMoreItems,

    // Cleanup
    destroy(): void {
      observer?.disconnect();
      resizeObserver?.disconnect();
      sentinel?.remove();
      container.removeEventListener('scroll', onUserScroll);
      if (idleTimeout) {
        clearTimeout(idleTimeout);
      }
      entityManager.clear();
    }
  };

  return scroller;
}

// Convenient presets
export const SCROLLER_PRESETS = {
  CHAT: {
    pageSize: 30,
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