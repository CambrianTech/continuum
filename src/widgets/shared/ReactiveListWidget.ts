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

  /** Backend preference for data loading.
   *  'auto' (default) = local-first with server fallback.
   *  'stale-while-revalidate' = return cached instantly, refresh from server in background.
   *  'server' = always fetch from server. */
  protected get loadBackend(): 'auto' | 'server' | 'local' | 'stale-while-revalidate' { return 'auto'; }

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
        <div
          class="${this.containerClass}"
          role="listbox"
          aria-label=${this.listTitle}
        >
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
      // ARIA listbox semantics (#1099 phase 2 + 3a). The container has
      // role="listbox"; each item is role="option". Roving tabindex
      // (only the active item gets tabindex=0, others -1) is managed
      // here for initial render and updated dynamically by
      // syncSelection() after every Lit update + onListKeydown after
      // arrow-key navigation.
      div.setAttribute('role', 'option');
      const isSel = this.isItemIdSelected(item.id);
      div.tabIndex = isSel ? 0 : -1;
      const label = this.getItemLabel(item);
      if (label) div.setAttribute('aria-label', label);
      div.setAttribute('aria-selected', String(isSel));
      render(this.renderItem(item), div);
      div.addEventListener('click', (e) => {
        e.stopPropagation();
        this.onItemClick(item);
      });
      // Enter or Space activates the item — same effect as a mouse click.
      // The click handler above already handles selection updates.
      div.addEventListener('keydown', (e: KeyboardEvent) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          e.stopPropagation();
          this.onItemClick(item);
        }
      });
      return div;
    };
  }

  /**
   * Accessible name for a list item. Default uses `displayName` or `name`
   * fields if present on the entity, otherwise empty (which omits the
   * aria-label and lets the screen reader fall back to the rendered
   * text content). Subclasses override to provide a richer label —
   * for example "<room name>, <member count> members".
   */
  protected getItemLabel(item: T): string {
    const e = item as unknown as { displayName?: string; name?: string };
    return e.displayName ?? e.name ?? '';
  }

  /**
   * Keyboard navigation handler attached to the listbox container in
   * `firstUpdated()`. ArrowDown/Up move focus to the next/previous
   * `.list-item`, Home/End jump to first/last, Enter/Space activate.
   * Updates roving tabindex so only the focused item is in the Tab
   * order (others get tabindex=-1) — keeps the list a single tab stop
   * instead of one per item.
   */
  private onListKeydown = (e: KeyboardEvent): void => {
    const items = Array.from(
      this.shadowRoot?.querySelectorAll<HTMLElement>(`.${this.containerClass} > .list-item`) ?? []
    );
    if (items.length === 0) return;

    const active = this.shadowRoot?.activeElement as HTMLElement | null;
    const currentIdx = active ? items.indexOf(active) : -1;

    let nextIdx: number | null = null;
    switch (e.key) {
      case 'ArrowDown':
        nextIdx = currentIdx < 0 ? 0 : Math.min(currentIdx + 1, items.length - 1);
        break;
      case 'ArrowUp':
        nextIdx = currentIdx < 0 ? items.length - 1 : Math.max(currentIdx - 1, 0);
        break;
      case 'Home':
        nextIdx = 0;
        break;
      case 'End':
        nextIdx = items.length - 1;
        break;
      default:
        return;
    }
    if (nextIdx !== null) {
      e.preventDefault();
      // Roving tabindex: only the about-to-be-focused item is in the
      // Tab order. Others step out so Tab from outside the list lands
      // on this one item.
      items.forEach((el, i) => { el.tabIndex = i === nextIdx ? 0 : -1; });
      items[nextIdx].focus();
    }
  };

  protected override firstUpdated(): void {
    super.firstUpdated();
    const container = this.shadowRoot?.querySelector(`.${this.containerClass}`);
    container?.addEventListener('keydown', this.onListKeydown as EventListener);
  }

  /**
   * After every Lit re-render, walk the rendered `.list-item` wrappers
   * and update `aria-selected` + the roving `tabindex` to reflect the
   * subclass's selection state. The visual `.active` class is already
   * reactive via Lit (subclasses re-render their inner template); this
   * hook keeps the ARIA attributes on the static EntityScroller-managed
   * outer wrapper in sync without re-rendering the wrapper.
   *
   * If no item is currently selected (e.g., first load before any
   * click), the first item gets tabindex=0 so the list remains a
   * tab stop. Otherwise the selected item gets tabindex=0, others -1.
   */
  protected override updated(changed: Map<string, unknown>): void {
    super.updated(changed);
    this.syncListSelection();
  }

  private syncListSelection(): void {
    const items = this.shadowRoot?.querySelectorAll<HTMLElement>(
      `.${this.containerClass} > .list-item`
    );
    if (!items || items.length === 0) return;
    let selectedFound = false;
    items.forEach(item => {
      const id = item.dataset.id;
      if (!id) return;
      const sel = this.isItemIdSelected(id);
      item.setAttribute('aria-selected', String(sel));
      item.tabIndex = sel ? 0 : -1;
      if (sel) selectedFound = true;
    });
    if (!selectedFound && items[0]) {
      items[0].tabIndex = 0;
    }
  }

  /**
   * Whether an item with the given id is the currently-selected one.
   * Base implementation uses `this.selectedId`. Subclasses with their
   * own selection state override this — RoomList uses `currentRoomId`,
   * UserList uses `_selectedUserId`. Drives both `aria-selected` and
   * the roving tabindex.
   */
  protected isItemIdSelected(id: string): boolean {
    return id === this.selectedId;
  }

  protected getLoadFunction(): LoadFn<T> {
    return async (cursor?: string, limit?: number) => {
      const result = await DataList.execute<T>({
          collection: this.collection,
          filter: this.loadFilter,
          orderBy: this.orderBy,
          limit: limit ?? this.pageSize,
          dbHandle: 'default',
          backend: this.loadBackend,
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
