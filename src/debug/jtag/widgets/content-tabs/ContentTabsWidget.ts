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
 */

import { BaseWidget } from '../shared/BaseWidget';
import { contentState, type ContentStateData } from '../../system/state/ContentStateService';
import { pageState } from '../../system/state/PageStateService';
import { Commands } from '../../system/core/shared/Commands';
import type { StateContentSwitchParams, StateContentSwitchResult } from '../../commands/state/content/switch/shared/StateContentSwitchTypes';
import type { StateContentCloseParams, StateContentCloseResult } from '../../commands/state/content/close/shared/StateContentCloseTypes';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

export interface TabInfo {
  id: string;
  label: string;
  active: boolean;
  closeable?: boolean;
  entityId?: string;
  contentType?: string;
}

export class ContentTabsWidget extends BaseWidget {
  private tabs: TabInfo[] = [];
  private unsubscribe?: () => void;

  constructor() {
    super({
      widgetName: 'ContentTabsWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    verbose() && console.log('📋 ContentTabsWidget: Subscribing to contentState...');

    // Subscribe to global contentState - React pattern
    this.unsubscribe = contentState.subscribe((state) => {
      this.updateFromContentState(state);
    });

    verbose() && console.log('✅ ContentTabsWidget: Subscribed');
  }

  /**
   * Update from global contentState - the ONLY source of truth
   */
  private updateFromContentState(state: ContentStateData): void {
    verbose() && console.log('📋 ContentTabsWidget: State update', state.openItems.length, 'items');

    // Convert to TabInfo format
    this.tabs = state.openItems.map(item => ({
      id: item.id,
      label: item.title || item.type,
      active: item.id === state.currentItemId,
      closeable: true,
      entityId: item.entityId,
      contentType: item.type
    }));

    this.renderWidget();
  }

  protected async renderWidget(): Promise<void> {
    const styles = `
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
    `;

    const tabsHTML = this.tabs.length === 0
      ? '<div class="empty-state">No content tabs</div>'
      : this.tabs.map(tab => `
          <div class="content-tab ${tab.active ? 'active' : ''}" data-tab-id="${tab.id}">
            <span class="tab-label">${tab.label}</span>
            ${tab.closeable ? `<span class="tab-close" data-close-tab="${tab.id}">×</span>` : ''}
          </div>
        `).join('');

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      <div class="content-tabs-container">${tabsHTML}</div>
    `;

    this.setupEventListeners();
  }

  private setupEventListeners(): void {
    const tabs = this.shadowRoot?.querySelectorAll('.content-tab');
    if (!tabs) return;

    tabs.forEach((tabElement) => {
      const tab = tabElement as HTMLElement;
      const tabId = tab.dataset.tabId;

      tab.addEventListener('click', (event) => {
        const target = event.target as HTMLElement;

        if (target.classList.contains('tab-close') || target.hasAttribute('data-close-tab')) {
          if (tabId) this.handleTabClose(tabId);
          event.stopPropagation();
          return;
        }

        if (tabId) this.handleTabClick(tabId);
      });
    });
  }

  /**
   * Handle tab click - update global state immediately
   */
  private handleTabClick(tabId: string): void {
    // Skip if already current
    if (contentState.currentItemId === tabId) return;

    // Find the tab to get its type/entityId
    const tab = this.tabs.find(t => t.id === tabId);
    if (!tab) return;

    // Update global state - triggers re-render via subscription
    contentState.setCurrent(tabId as UUID);

    // Update pageState for MainWidget view switching
    pageState.setContent(tab.contentType || '', tab.entityId, undefined);

    // Persist to DB in background
    const userId = this.userState?.userId;
    if (userId) {
      Commands.execute<StateContentSwitchParams, StateContentSwitchResult>('state/content/switch', {
        userId: userId as UUID,
        contentItemId: tabId as UUID
      }).catch(err => console.error('ContentTabsWidget: Failed to persist tab switch:', err));
    }
  }

  /**
   * Handle tab close - update global state immediately
   */
  private handleTabClose(tabId: string): void {
    // Remove from global state - triggers re-render via subscription
    contentState.removeItem(tabId as UUID);

    // Get new current item for pageState update
    const newCurrent = contentState.currentItem;
    if (newCurrent) {
      pageState.setContent(newCurrent.type, newCurrent.entityId, undefined);
    }

    // Persist to DB in background
    const userId = this.userState?.userId;
    if (userId) {
      Commands.execute<StateContentCloseParams, StateContentCloseResult>('state/content/close', {
        userId: userId as UUID,
        contentItemId: tabId as UUID
      }).catch(err => console.error('ContentTabsWidget: Failed to persist tab close:', err));
    }
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.unsubscribe?.();
    verbose() && console.log('🧹 ContentTabsWidget: Cleanup complete');
  }
}
