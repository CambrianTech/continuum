/**
 * ReactiveListWidget - List control with header/item/footer pattern
 *
 * Standard list control pattern:
 * - header: Title, count badge, search, etc.
 * - item: Repeating item template (populated by EntityScroller)
 * - footer: Optional - input box for chat, pagination controls, etc.
 *
 * Subclasses implement:
 * - collection: string (entity collection name)
 * - renderItem(item: T): TemplateResult (each list item)
 * - renderHeader(): TemplateResult (optional, has default)
 * - renderFooter(): TemplateResult (optional, empty by default)
 *
 * SCSS Support:
 * ```typescript
 * import { styles as externalStyles } from './my-widget.styles';
 * static override styles = [ReactiveListWidget.styles, unsafeCSS(externalStyles)];
 * ```
 */

import {
  ReactiveEntityScrollerWidget,
  html,
  css,
  reactive,
  type TemplateResult,
  type CSSResultGroup
} from './ReactiveEntityScrollerWidget';
import { render, unsafeCSS, nothing } from 'lit';
import { SCROLLER_PRESETS, type RenderFn, type LoadFn, type ScrollerConfig, type RenderContext } from './EntityScroller';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { Commands } from '../../system/core/shared/Commands';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import { BaseEntity } from '../../system/data/entities/BaseEntity';

import { DataList } from '../../commands/data/list/shared/DataListTypes';
// Re-export for subclasses
export { html, css, unsafeCSS, nothing, reactive, type TemplateResult, type CSSResultGroup };

export interface ReactiveListConfig {
  widgetName: string;
  styles?: string;
}

/**
 * Base class for list widgets with header/item/footer sections
 */
export abstract class ReactiveListWidget<T extends BaseEntity> extends ReactiveEntityScrollerWidget<T> {

  // === REQUIRED ===
  abstract readonly collection: string;
  abstract renderItem(item: T): TemplateResult;

  // === REACTIVE STATE ===
  @reactive() protected selectedId: string | null = null;

  // === OPTIONAL CONFIGURATION ===

  /** Database-level filter (passed to data/list command) */
  protected get loadFilter(): Record<string, unknown> { return {}; }

  /** Client-side filter - override to filter loaded items (e.g., by tags) */
  protected shouldAddEntity(_item: T): boolean { return true; }

  protected get orderBy(): Array<{ field: string; direction: 'asc' | 'desc' }> {
    return [{ field: 'name', direction: 'asc' }];
  }
  protected get scrollerPreset(): ScrollerConfig { return SCROLLER_PRESETS.LIST; }
  protected get containerClass(): string { return 'list-body'; }
  protected get pageSize(): number { return 100; }

  /** List title shown in header */
  protected get listTitle(): string { return 'Items'; }

  // === CONSTRUCTOR ===

  constructor(config: ReactiveListConfig) {
    super({ widgetName: config.widgetName, styles: config.styles });
  }

  // === HEADER / FOOTER SECTIONS ===

  /**
   * Render list header (title + count by default)
   * Override for custom header (search box, filters, etc.)
   */
  protected renderHeader(): TemplateResult {
    return html`
      <div class="list-header">
        <span class="list-title">${this.listTitle}</span>
        <span class="list-count">${this.entityCount}</span>
      </div>
    `;
  }

  /**
   * Render list footer (empty by default)
   * Override to add input box (chat), pagination, etc.
   */
  protected renderFooter(): TemplateResult | typeof nothing {
    return nothing;
  }

  // === MAIN RENDER - Composes header/body/footer ===

  override render(): TemplateResult {
    return html`
      <div class="list-widget">
        ${this.renderHeader()}
        <div class="${this.containerClass}">
          <!-- EntityScroller populates items here -->
        </div>
        ${this.renderFooter()}
      </div>
    `;
  }


  // === ENTITY SCROLLER IMPLEMENTATION ===

  protected getRenderFunction(): RenderFn<T> {
    return (item: T, _context: RenderContext<T>) => {
      const div = document.createElement('div');
      div.className = 'list-item';
      div.dataset.id = item.id;
      render(this.renderItem(item), div);
      div.addEventListener('click', (e) => {
        e.stopPropagation();
        this.onItemClick(item);
      });
      return div;
    };
  }

  protected getLoadFunction(): LoadFn<T> {
    return async (cursor?: string, limit?: number) => {
      const result = await DataList.execute<T>({
          collection: this.collection,
          filter: this.loadFilter,
          orderBy: this.orderBy,
          limit: limit ?? this.pageSize
        }
      );
      if (!result?.success) {
        throw new Error(`Failed to load ${this.collection}: ${result?.error ?? 'Unknown error'}`);
      }
      // Apply client-side filter via shouldAddEntity
      const items = (result.items ?? []).filter(item => this.shouldAddEntity(item));
      return { items, hasMore: false, nextCursor: undefined };
    };
  }

  protected getScrollerPreset(): ScrollerConfig { return this.scrollerPreset; }
  protected getContainerSelector(): string { return `.${this.containerClass}`; }
  protected getEntityCollection(): string { return this.collection; }

  // === HOOKS ===

  protected onItemClick(item: T): void {
    this.selectedId = item.id;
  }

  protected isSelected(item: T): boolean {
    return item.id === this.selectedId;
  }

  // === MINIMAL BASE STYLES ===

  static override styles = css`
    :host {
      display: flex;
      flex-direction: column;
      height: 100%;
    }
    .list-widget {
      display: flex;
      flex-direction: column;
      height: 100%;
    }
    .list-body {
      flex: 1;
      overflow-y: auto;
      min-height: 0;
    }
  ` as CSSResultGroup;
}
