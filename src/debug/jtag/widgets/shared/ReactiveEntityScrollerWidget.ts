/**
 * ReactiveEntityScrollerWidget - Combines ReactiveWidget with EntityScroller
 *
 * Best of both worlds:
 * - Lit's reactive rendering (efficient diffing, declarative templates)
 * - EntityScroller's virtualized list management (infinite scroll, CRUD sync)
 *
 * Migration path from EntityScrollerWidget:
 * 1. Change extends to ReactiveEntityScrollerWidget
 * 2. Convert manual state to @reactive() decorators
 * 3. Replace renderTemplate() with render() returning html``
 * 4. Use createEffect() for side effects instead of watch()
 */

import { ReactiveWidget, html, css, type TemplateResult, type CSSResultGroup, reactive, type EffectCleanup } from './ReactiveWidget';
import { createScroller, type RenderFn, type LoadFn, type EntityScroller, type ScrollerConfig } from './EntityScroller';
import { createEntityCrudHandler } from '../../commands/data/shared/DataEventUtils';
import { BaseEntity } from '../../system/data/entities/BaseEntity';

// Re-export for subclasses
export { html, css, reactive, type TemplateResult, type CSSResultGroup };

export interface ReactiveScrollerConfig {
  widgetName: string;
  styles?: string;
  enableAI?: boolean;
  enableDatabase?: boolean;
}

export abstract class ReactiveEntityScrollerWidget<T extends BaseEntity> extends ReactiveWidget {
  protected scroller?: EntityScroller<T>;
  private _unsubscribeEvents?: () => void;
  private _scrollerInitialized = false;
  private _pendingUpdate = false;

  /**
   * Batched requestUpdate() - prevents cascading re-renders from rapid CRUD events.
   * Multiple calls within the same microtask are coalesced into a single update.
   */
  private batchedUpdate(): void {
    if (this._pendingUpdate) return;
    this._pendingUpdate = true;
    queueMicrotask(() => {
      this._pendingUpdate = false;
      this.requestUpdate();
    });
  }

  constructor(config: ReactiveScrollerConfig) {
    super();
    // Store config for later use
    (this as unknown as { _scrollerConfig: ReactiveScrollerConfig })._scrollerConfig = config;
  }

  // === Abstract methods for subclasses ===

  /** Render function for each entity in the list */
  protected abstract getRenderFunction(): RenderFn<T>;

  /** Load function for fetching entities (pagination support) */
  protected abstract getLoadFunction(): LoadFn<T>;

  /** Scroller configuration (direction, page size, etc) */
  protected abstract getScrollerPreset(): ScrollerConfig;

  /** CSS selector for the scroller container element */
  protected abstract getContainerSelector(): string;

  /** Entity collection name for CRUD event subscriptions */
  protected abstract getEntityCollection(): string;

  // === Optional overrides for filtering ===

  /** Filter which entities should be added (default: all) */
  protected shouldAddEntity(_entity: T): boolean {
    return true;
  }

  /** Filter which entities should be updated (default: all) */
  protected shouldUpdateEntity(_id: string, _entity: T): boolean {
    return true;
  }

  /** Filter which entities should be removed (default: all) */
  protected shouldRemoveEntity(_id: string): boolean {
    return true;
  }

  // === Lifecycle ===

  override connectedCallback(): void {
    super.connectedCallback();
    this.setupEntityEventSubscriptions();
  }

  override disconnectedCallback(): void {
    this.cleanupScroller();
    super.disconnectedCallback();
  }

  protected override firstUpdated(): void {
    super.firstUpdated();
    // Setup scroller after first render when DOM exists
    // Use queueMicrotask to avoid blocking the render thread
    queueMicrotask(() => {
      this.setupEntityScroller();
    });
  }

  // === EntityScroller Setup ===

  private async setupEntityScroller(): Promise<void> {
    if (this._scrollerInitialized) return;

    const container = this.shadowRoot?.querySelector(this.getContainerSelector()) as HTMLElement;
    if (!container) {
      console.error(`${this.constructor.name}: Container "${this.getContainerSelector()}" not found`);
      return;
    }

    // Dynamic load function wrapper - gets fresh state each call
    const dynamicLoadFn: LoadFn<T> = (cursor?: string, limit?: number) => {
      const loadFn = this.getLoadFunction();
      return loadFn(cursor, limit);
    };

    this.scroller = createScroller(
      container,
      this.getRenderFunction(),
      dynamicLoadFn,
      this.getScrollerPreset()
    );

    // Load initial data
    await this.scroller.load();
    this._scrollerInitialized = true;

    // Trigger reactive update for entity count
    this.requestUpdate();
  }

  private setupEntityEventSubscriptions(): void {
    if (this._unsubscribeEvents) return; // Already subscribed

    const collection = this.getEntityCollection();

    this._unsubscribeEvents = createEntityCrudHandler<T>(
      collection,
      {
        add: (entity: T) => {
          if (this.shouldAddEntity(entity)) {
            this.scroller?.addWithAutoScroll(entity);
            this.batchedUpdate(); // Batched - coalesces rapid adds
          }
        },
        update: (id: string, entity: T) => {
          if (this.shouldUpdateEntity(id, entity)) {
            this.scroller?.update(id, entity);
            this.batchedUpdate(); // Batched
          }
        },
        remove: (id: string) => {
          if (this.shouldRemoveEntity(id)) {
            this.scroller?.remove(id);
            this.batchedUpdate(); // Batched
          }
        },
        clear: () => {
          this.scroller?.clear();
          this.batchedUpdate(); // Batched
        }
      }
    );
  }

  private cleanupScroller(): void {
    this._unsubscribeEvents?.();
    this._unsubscribeEvents = undefined;
    this.scroller?.destroy();
    this.scroller = undefined;
    this._scrollerInitialized = false;
  }

  // === Convenience methods ===

  /** Get current entity count */
  protected get entityCount(): number {
    return this.scroller?.entities().length ?? 0;
  }

  /** Get all entities */
  protected get entities(): readonly T[] {
    return this.scroller?.entities() ?? [];
  }

  /** Refresh the scroller (reload data) */
  protected async refresh(): Promise<void> {
    await this.scroller?.refresh();
    this.batchedUpdate(); // Batched for consistency
  }

  /** Clear all entities */
  protected clear(): void {
    this.scroller?.clear();
    this.batchedUpdate(); // Batched for consistency
  }

  /** Scroll to end (useful for chat) */
  protected scrollToEnd(): void {
    this.scroller?.scrollToEnd();
  }

  /** Reinitialize scroller (e.g., after container change) */
  protected async reinitializeScroller(): Promise<void> {
    this.scroller?.destroy();
    this.scroller = undefined;
    this._scrollerInitialized = false;
    await this.setupEntityScroller();
  }
}
