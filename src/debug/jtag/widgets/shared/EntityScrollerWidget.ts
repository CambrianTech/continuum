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
    this.scroller = createScroller(
      container,
      this.getRenderFunction(),
      this.getLoadFunction(),
      this.getScrollerPreset()
    );

    // Load initial data
    await this.scroller.load();
    console.log(`✅ ${this.constructor.name}: Initialized EntityScroller with automatic deduplication`);

    // Update count after initial load
    this.updateEntityCount();
  }

  /**
   * Unified CRUD event subscriptions - eliminates duplicate patterns across widgets
   */
  private async setupEntityEventSubscriptions(): Promise<void> {
    console.log(`🔧 ${this.constructor.name}: Setting up CRUD event subscriptions...`);

    try {
      const collection = this.getEntityCollection();
      console.log(`🎧 ${this.constructor.name}: Setting up unified CRUD subscriptions for ${collection}`);

      // Single subscription for ALL CRUD operations (create, update, delete)
      this.unsubscribeEvents = createEntityCrudHandler<T>(
        collection,
        {
          add: (entity: T) => {
            this.scroller?.add(entity);
            this.updateEntityCount();
          },
          update: (id: string, entity: T) => {
            this.scroller?.update(id, entity);
            this.updateEntityCount();
          },
          remove: (id: string) => {
            this.scroller?.remove(id);
            this.updateEntityCount();
          }
        }
      );

      console.log(`✅ ${this.constructor.name}: CRUD subscriptions active`);
    } catch (error) {
      console.error(`❌ ${this.constructor.name}: Failed to set up event subscriptions:`, error);
    }
  }

  protected override getEntityCount(): number {
    return this.scroller?.entities().length ?? 0;
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