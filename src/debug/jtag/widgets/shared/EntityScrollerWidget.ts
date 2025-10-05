/**
 * EntityScrollerWidget - Base class for widgets using EntityScroller pattern
 *
 * Eliminates ~300 lines of duplicate EntityScroller setup, event subscriptions,
 * and cleanup code across ChatWidget, RoomListWidget, and UserListWidget.
 */

import { EntityListWidget } from './EntityListWidget';
import { createScroller, type RenderFn, type LoadFn, type EntityScroller } from './EntityScroller';
import type { WidgetConfig } from './BaseWidget';
import { createEntityCrudHandler } from '../../commands/data/shared/DataEventUtils';
import type { ScrollerConfig } from './EntityScroller';

import { BaseEntity } from '../../system/data/entities/BaseEntity';

export abstract class EntityScrollerWidget<T extends BaseEntity> extends EntityListWidget<T> {
  protected scroller?: EntityScroller<T>;
  private unsubscribeEvents?: () => void;

  constructor(config: WidgetConfig) {
    super(config);
  }

  // Abstract methods that each widget must implement
  protected abstract getRenderFunction(): RenderFn<T>;
  protected abstract getLoadFunction(): LoadFn<T>;
  protected abstract getScrollerPreset(): ScrollerConfig;
  protected abstract getContainerSelector(): string;
  protected abstract getEntityCollection(): string;

  /**
   * Default template implementation - automatically generates CSS class from widget name
   * Can be optionally overridden by specific widgets if needed
   */
  protected renderTemplate(): string {
    const cssClass = this.generateEntityCssClass();
    return `
      <div class="entity-list-container">
        ${this.renderHeader()}

        <div class="entity-list-body ${cssClass}">
          <!-- EntityScroller will populate this container -->
        </div>

        ${this.renderFooter()}
      </div>
    `;
  }

  /**
   * Generate CSS class name from widget name
   * RoomListWidget -> room-list
   * UserListWidget -> user-list
   */
  private generateEntityCssClass(): string {
    const widgetName = this.constructor.name;
    return widgetName
      .replace(/Widget$/, '')  // Remove 'Widget' suffix
      .replace(/([a-z])([A-Z])/g, '$1-$2')  // Convert camelCase to kebab-case
      .toLowerCase();
  }

  protected override async renderWidget(): Promise<void> {
    // Call parent renderWidget implementation (EntityListWidget)
    await super.renderWidget();

    // Now set up EntityScroller after DOM exists
    await this.setupEntityScroller();
  }

  protected override async onWidgetInitialize(): Promise<void> {
    console.log(`🔧 EntityScrollerWidget: Initializing ${this.constructor.name}...`);

    // Setup event subscriptions first - EntityScroller will be set up after template renders
    await this.setupEntityEventSubscriptions();

    console.log(`✅ EntityScrollerWidget: Initialized ${this.constructor.name}`);
  }

  /**
   * Unified EntityScroller setup - eliminates duplicate code across all widgets
   */
  private async setupEntityScroller(): Promise<void> {
    const container = this.shadowRoot.querySelector(this.getContainerSelector()) as HTMLElement;
    if (!container) {
      console.error(`❌ ${this.constructor.name}: Could not find container "${this.getContainerSelector()}"`);
      return;
    }

    // Create scroller with widget-specific functions and preset
    // For dynamic widgets (like ChatWidget), we need to call getLoadFunction() fresh each time
    const dynamicLoadFn = (cursor?: string, limit?: number) => {
      const loadFn = this.getLoadFunction();
      return loadFn(cursor, limit);
    };

    this.scroller = createScroller(
      container,
      this.getRenderFunction(),
      dynamicLoadFn, // Dynamic wrapper - gets fresh roomId each time
      this.getScrollerPreset()
    );

    // Load initial data
    await this.scroller.load();
    console.log(`✅ ${this.constructor.name}: Initialized EntityScroller with automatic deduplication`);

    // Update count after initial load
    this.updateEntityCount();
  }

  /**
   * Abstract filtering hooks - subclasses override to control CRUD event processing
   */
  protected shouldAddEntity(entity: T): boolean {
    // Default: accept all entities (maintains backward compatibility)
    return true;
  }

  protected shouldUpdateEntity(id: string, entity: T): boolean {
    // Default: accept all updates (maintains backward compatibility)
    return true;
  }

  protected shouldRemoveEntity(id: string): boolean {
    // Default: accept all removals (maintains backward compatibility)
    return true;
  }

  /**
   * Unified CRUD event subscriptions with filtering hooks for subclass control
   */
  private async setupEntityEventSubscriptions(): Promise<void> {
    console.log(`🔧 ${this.constructor.name}: Setting up CRUD event subscriptions...`);

    try {
      const collection = this.getEntityCollection();
      console.log(`🎧 ${this.constructor.name}: Setting up unified CRUD subscriptions for ${collection}`);

      // Single subscription for ALL CRUD operations (create, update, delete)
      // Now with filtering hooks for subclass control
      this.unsubscribeEvents = createEntityCrudHandler<T>(
        collection,
        {
          add: (entity: T) => {
            // CRITICAL: Allow subclass to filter which entities should be added
            if (this.shouldAddEntity(entity)) {
              console.log(`🔧 CLAUDE-FIX-${Date.now()}: ${this.constructor.name} adding entity:`, entity.id);
              this.scroller?.add(entity);
              this.updateEntityCount();
            } else {
              console.log(`🔧 CLAUDE-FIX-${Date.now()}: ${this.constructor.name} filtered out entity:`, entity.id);
            }
          },
          update: (id: string, entity: T) => {
            // CRITICAL: Allow subclass to filter which entities should be updated
            if (this.shouldUpdateEntity(id, entity)) {
              console.log(`🔧 CLAUDE-FIX-${Date.now()}: ${this.constructor.name} updating entity:`, id);
              this.scroller?.update(id, entity);
              this.updateEntityCount();
            } else {
              console.log(`🔧 CLAUDE-FIX-${Date.now()}: ${this.constructor.name} filtered out update:`, id);
            }
          },
          remove: (id: string) => {
            // CRITICAL: Allow subclass to filter which entities should be removed
            if (this.shouldRemoveEntity(id)) {
              console.log(`🔧 CLAUDE-FIX-${Date.now()}: ${this.constructor.name} removing entity:`, id);
              this.scroller?.remove(id);
              this.updateEntityCount();
            } else {
              console.log(`🔧 CLAUDE-FIX-${Date.now()}: ${this.constructor.name} filtered out removal:`, id);
            }
          }
        }
      );

      console.log(`✅ ${this.constructor.name}: CRUD subscriptions active with filtering hooks`);
    } catch (error) {
      console.error(`❌ ${this.constructor.name}: Failed to set up event subscriptions:`, error);
    }
  }

  protected override getEntityCount(): number {
    return this.scroller?.entities().length ?? 0;
  }

  /**
   * Clear all entities from the scroller
   */
  public clear(): void {
    this.scroller?.clear();
    this.updateEntityCount();
  }

  protected override async onWidgetCleanup(): Promise<void> {
    // Clean up event subscriptions
    this.unsubscribeEvents?.();
    this.unsubscribeEvents = undefined;

    // Clean up EntityScroller
    this.scroller?.destroy();
    this.scroller = undefined;

    console.log(`🧹 ${this.constructor.name}: EntityScroller cleanup complete`);
  }
}