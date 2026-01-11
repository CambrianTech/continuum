/**
 * ContentTabsWidget - React-style subscriber to global content state
 *
 * ARCHITECTURE:
 * 1. Subscribes to contentState (global singleton)
 * 2. Renders tabs from contentState.openItems
 * 3. On click: updates contentState → triggers re-render
 * 4. On close: updates contentState → triggers re-render
 *
 * NO events, NO parent calls, just shared state.
 *
 * Uses ReactiveWidget with Lit templates for efficient rendering.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { contentState, type ContentStateData } from '../../system/state/ContentStateService';
import { ContentService } from '../../system/state/ContentService';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

export interface TabInfo {
  id: string;
  label: string;
  active: boolean;
  closeable?: boolean;
  entityId?: string;
  uniqueId?: string;  // Human-readable ID for URLs
  contentType?: string;
}

export class ContentTabsWidget extends ReactiveWidget {
  // Static styles
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(`
      .content-tabs-container {
        display: flex;
        gap: 0;
        align-items: flex-end;
        flex: 1;
        overflow-x: auto;
        overflow-y: hidden;
        margin-bottom: -1px;
        padding-bottom: 1px;
        scrollbar-width: none;
        -ms-overflow-style: none;
      }

      .content-tabs-container::-webkit-scrollbar {
        display: none;
      }

      .content-tab {
        padding: 8px 16px;
        background: rgba(15, 20, 25, 0.6);
        border: 1px solid rgba(0, 212, 255, 0.2);
        border-bottom: 1px solid rgba(0, 212, 255, 0.15);
        border-radius: 6px 6px 0 0;
        color: rgba(0, 212, 255, 0.6);
        font-size: 13px;
        font-weight: 500;
        cursor: pointer;
        white-space: nowrap;
        user-select: none;
        transition: all 0.15s ease;
        display: flex;
        align-items: center;
        gap: 8px;
        margin-right: 2px;
        position: relative;
      }

      .content-tab:hover {
        background: rgba(15, 20, 25, 0.8);
        border-color: rgba(0, 212, 255, 0.4);
        color: rgba(0, 212, 255, 0.9);
      }

      .content-tab.active {
        background: rgba(15, 20, 25, 0.95);
        border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
        border-bottom-color: transparent;
        color: var(--content-accent, #00d4ff);
        font-weight: 600;
        z-index: 1;
      }

      .tab-close {
        width: 16px;
        height: 16px;
        display: flex;
        align-items: center;
        justify-content: center;
        border-radius: 50%;
        font-size: 10px;
        opacity: 0.6;
        transition: all 0.2s ease;
      }

      .tab-close:hover {
        opacity: 1;
        background: rgba(255, 50, 50, 0.2);
        color: #ff5050;
      }

      .empty-state {
        padding: 8px 16px;
        color: rgba(0, 212, 255, 0.5);
        font-size: 12px;
        font-style: italic;
      }
    `)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private tabs: TabInfo[] = [];

  constructor() {
    super({
      widgetName: 'ContentTabsWidget'
    });
  }

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();

    // Subscribe to global contentState - React pattern
    this.createMountEffect(() => {
      const unsubscribe = contentState.subscribe((state) => {
        this.updateFromContentState(state);
      });
      return () => unsubscribe();
    });
  }

  /**
   * Update from global contentState - the ONLY source of truth
   */
  private updateFromContentState(state: ContentStateData): void {
    // Convert to TabInfo format
    this.tabs = state.openItems.map(item => ({
      id: item.id,
      label: item.title || item.type,
      active: item.id === state.currentItemId,
      closeable: true,
      entityId: item.entityId,
      uniqueId: item.uniqueId,  // For human-readable URLs
      contentType: item.type
    }));
    this.requestUpdate();
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    if (this.tabs.length === 0) {
      return html`
        <div class="content-tabs-container">
          <div class="empty-state">No content tabs</div>
        </div>
      `;
    }

    return html`
      <div class="content-tabs-container">
        ${this.tabs.map(tab => html`
          <div class="content-tab ${tab.active ? 'active' : ''}"
               @click=${(e: Event) => this.handleTabClick(e, tab)}>
            <span class="tab-label">${tab.label}</span>
            ${tab.closeable ? html`
              <span class="tab-close" @click=${(e: Event) => this.handleTabClose(e, tab)}>×</span>
            ` : ''}
          </div>
        `)}
      </div>
    `;
  }

  // === Event Handlers ===

  /**
   * Handle tab click - delegate to ContentService
   */
  private handleTabClick(event: Event, tab: TabInfo): void {
    if (this.userState?.userId) {
      ContentService.setUserId(this.userState.userId as UUID);
    }
    ContentService.switchTo(tab.id);
  }

  /**
   * Handle tab close - delegate to ContentService
   */
  private handleTabClose(event: Event, tab: TabInfo): void {
    event.stopPropagation(); // Don't trigger tab click

    if (this.userState?.userId) {
      ContentService.setUserId(this.userState.userId as UUID);
    }
    ContentService.close(tab.id);
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
