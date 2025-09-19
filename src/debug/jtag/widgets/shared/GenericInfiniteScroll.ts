/**
 * Generic Infinite Scroll Implementation
 *
 * Reusable infinite scroll logic extracted from ChatWidget's proven implementation.
 * Can be used by any widget that needs cursor-based pagination.
 */

import type {
  InfiniteScrollConfig,
  PaginationState,
  InfiniteScrollCallbacks,
  LoadResult,
  DEFAULT_INFINITE_SCROLL_CONFIG
} from './InfiniteScrollTypes';

/**
 * Generic infinite scroll helper that works with any item type and cursor type
 */
export class GenericInfiniteScroll<TItem, TCursor = string> {
  private observer?: IntersectionObserver;
  private sentinel?: HTMLElement;
  private scrollContainer?: HTMLElement;
  private state: PaginationState<TCursor>;

  constructor(
    private readonly config: InfiniteScrollConfig,
    private readonly callbacks: InfiniteScrollCallbacks<TItem, TCursor>
  ) {
    this.state = {
      hasMore: true,
      isLoading: false
    };
  }

  /**
   * Initialize with container and initial items
   */
  initialize(scrollContainer: HTMLElement, initialItems: TItem[] = []): void {
    this.scrollContainer = scrollContainer;
    this.createSentinel();
    this.setupIntersectionObserver();

    if (initialItems.length > 0) {
      this.initializeWithItems(initialItems);
    }
  }

  /**
   * Initialize pagination state with first batch of items
   */
  private initializeWithItems(items: TItem[]): void {
    if (items.length === 0) return;

    // Sort items using provided comparator
    const sortedItems = items.slice().sort((a, b) =>
      this.callbacks.compareCursors(
        this.callbacks.getCursor(a),
        this.callbacks.getCursor(b)
      )
    );

    this.state = {
      hasMore: true,
      isLoading: false,
      oldestCursor: this.callbacks.getCursor(sortedItems[sortedItems.length - 1]),
      newestCursor: this.callbacks.getCursor(sortedItems[0])
    };
  }

  /**
   * Create invisible sentinel element for intersection detection
   */
  private createSentinel(): void {
    if (!this.scrollContainer) return;

    this.sentinel = document.createElement('div');
    this.sentinel.style.cssText = 'height: 1px; width: 100%; position: absolute; top: 0; pointer-events: none; opacity: 0;';
    this.sentinel.setAttribute('data-infinite-scroll-sentinel', 'true');

    this.scrollContainer.insertBefore(this.sentinel, this.scrollContainer.firstChild);
  }

  /**
   * Set up intersection observer
   */
  private setupIntersectionObserver(): void {
    if (!this.sentinel || !this.config.enabled) return;

    this.observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        this.handleIntersection(entry);
      }
    }, {
      root: this.scrollContainer,
      threshold: this.config.threshold,
      rootMargin: this.config.rootMargin
    });

    this.observer.observe(this.sentinel);
  }

  /**
   * Handle intersection observer events
   */
  private handleIntersection(entry: IntersectionObserverEntry): void {
    const isIntersecting = entry.isIntersecting;
    const canLoadMore = this.state.hasMore && !this.state.isLoading;

    if (isIntersecting && canLoadMore) {
      this.loadOlderItems();
    }
  }

  /**
   * Load older items using cursor pagination
   */
  private async loadOlderItems(): Promise<TItem[]> {
    if (this.state.isLoading || !this.state.hasMore) {
      return [];
    }

    this.state = { ...this.state, isLoading: true };

    try {
      const result = await this.callbacks.loadItems(
        this.state.oldestCursor,
        this.config.pageSize
      );

      // Update state based on result
      const hasMore = result.hasMore || result.items.length === this.config.pageSize;

      if (result.items.length > 0) {
        const sortedItems = (result.items as TItem[]).slice().sort((a, b) =>
          this.callbacks.compareCursors(
            this.callbacks.getCursor(a),
            this.callbacks.getCursor(b)
          )
        );

        this.state = {
          hasMore,
          isLoading: false,
          oldestCursor: this.callbacks.getCursor(sortedItems[sortedItems.length - 1]),
          newestCursor: this.state.newestCursor // Keep existing newest
        };
      } else {
        this.state = { ...this.state, hasMore: false, isLoading: false };
      }

      return result.items.slice();
    } catch (error) {
      console.error('GenericInfiniteScroll: Failed to load items:', error);
      this.state = { ...this.state, isLoading: false };
      return [];
    }
  }

  /**
   * Prepend new items to container (for infinite scroll)
   */
  async prependItems(items: TItem[]): Promise<void> {
    if (!this.scrollContainer || items.length === 0) return;

    // Save scroll position
    const scrollHeight = this.scrollContainer.scrollHeight;
    const scrollTop = this.scrollContainer.scrollTop;

    // Create fragment with new items
    const fragment = document.createDocumentFragment();
    for (const item of items) {
      const element = this.callbacks.createItemElement(item);
      fragment.appendChild(element);
    }

    // Insert at beginning
    const firstChild = this.scrollContainer.firstElementChild;
    if (firstChild) {
      this.scrollContainer.insertBefore(fragment, firstChild);
    } else {
      this.scrollContainer.appendChild(fragment);
    }

    // Restore scroll position
    requestAnimationFrame(() => {
      const newScrollHeight = this.scrollContainer!.scrollHeight;
      const heightDifference = newScrollHeight - scrollHeight;
      this.scrollContainer!.scrollTop = scrollTop + heightDifference;

      // Reset intersection observer after DOM changes
      this.forceIntersectionCheck();
    });
  }

  /**
   * Force intersection observer to re-evaluate after DOM changes
   */
  private forceIntersectionCheck(): void {
    if (!this.sentinel || !this.scrollContainer || !this.observer) return;

    requestAnimationFrame(() => {
      if (this.sentinel && this.scrollContainer) {
        // Reposition sentinel
        this.sentinel.remove();
        this.scrollContainer.insertBefore(this.sentinel, this.scrollContainer.firstChild);

        // Reset observer
        requestAnimationFrame(() => {
          if (this.observer && this.sentinel) {
            this.observer.unobserve(this.sentinel);
            this.observer.observe(this.sentinel);
          }
        });
      }
    });
  }

  /**
   * Get current state
   */
  getState(): Readonly<PaginationState<TCursor>> {
    return this.state;
  }

  /**
   * Cleanup
   */
  destroy(): void {
    this.observer?.disconnect();
    this.sentinel?.remove();
    this.observer = undefined;
    this.sentinel = undefined;
    this.scrollContainer = undefined;
  }
}