/**
 * EntityListHeader — Reusable header for entity list widgets
 *
 * Shows title, count, and optional filter chips.
 * Emits 'filter-change' event when a filter is clicked.
 * Single component replacing 4+ inline copies.
 */

import { ReactiveWidget, html, css, reactive, type TemplateResult, type CSSResultGroup } from './ReactiveWidget';
import { nothing } from 'lit';

export interface FilterOption {
  id: string;
  label: string;
  icon?: string;
}

export class EntityListHeader extends ReactiveWidget {

  @reactive() title = 'Items';
  @reactive() count = 0;
  @reactive() activeFilter = 'all';
  @reactive() filters: FilterOption[] = [];

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
      }

      .header {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 10px 14px 6px;
      }

      .header-title {
        font-size: 14px;
        font-weight: 700;
        color: var(--content-primary, #e0e6ed);
      }

      .filter-chips {
        display: flex;
        gap: 4px;
        flex: 1;
      }

      .filter-chip {
        font-size: 10px;
        font-weight: 600;
        padding: 2px 8px;
        border-radius: 10px;
        border: 1px solid rgba(255,255,255,0.1);
        background: transparent;
        color: var(--content-tertiary, #5a6070);
        cursor: pointer;
        transition: all 0.15s;
        white-space: nowrap;
      }

      .filter-chip:hover {
        border-color: rgba(0, 212, 255, 0.3);
        color: var(--content-secondary, #8a92a5);
      }

      .filter-chip.active {
        background: rgba(0, 212, 255, 0.12);
        border-color: rgba(0, 212, 255, 0.4);
        color: var(--accent-primary, #00d4ff);
      }

      .chip-icon {
        margin-right: 2px;
      }

      .header-count {
        font-size: 13px;
        font-weight: 700;
        font-variant-numeric: tabular-nums;
        color: var(--accent-primary, #00d4ff);
        min-width: 20px;
        text-align: right;
      }
    `,
  ];

  private onFilterClick(filterId: string): void {
    this.activeFilter = filterId;
    this.dispatchEvent(new CustomEvent('filter-change', {
      detail: { filter: filterId },
      bubbles: true,
      composed: true,
    }));
  }

  protected override render(): TemplateResult {
    return html`
      <div class="header">
        <span class="header-title">${this.title}</span>
        ${this.filters.length > 0 ? html`
          <div class="filter-chips">
            ${this.filters.map(f => html`
              <button class="filter-chip ${this.activeFilter === f.id ? 'active' : ''}"
                @click=${() => this.onFilterClick(f.id)}>
                ${f.icon ? html`<span class="chip-icon">${f.icon}</span>` : nothing}${f.label}
              </button>
            `)}
          </div>
        ` : nothing}
        <span class="header-count">${this.count}</span>
      </div>
    `;
  }
}

if (!customElements.get('entity-list-header')) {
  customElements.define('entity-list-header', EntityListHeader);
}
