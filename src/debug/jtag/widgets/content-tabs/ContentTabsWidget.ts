/**
 * ContentTabsWidget - Dynamic content tabs for navigation
 *
 * Displays content tabs based on current context (rooms, threads, pages).
 * Emits tab-clicked events for parent widget to handle navigation.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';

export interface TabInfo {
  id: string;
  label: string;
  active: boolean;
  closeable?: boolean;
}

export class ContentTabsWidget extends BaseWidget {
  private tabs: TabInfo[] = [];

  constructor() {
    super({
      widgetName: 'ContentTabsWidget',
      template: undefined,  // Inline template
      styles: undefined,     // Inline styles
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('📋 ContentTabsWidget: Initializing content tabs...');

    // Subscribe to tab updates from parent or router
    Events.subscribe('tabs:update', (tabs: TabInfo[]) => {
      this.tabs = tabs;
      this.renderWidget();
    });

    console.log('✅ ContentTabsWidget: Initialized');
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
        /* Extend tabs below the header border to overlap it */
        margin-bottom: -1px;
        padding-bottom: 1px;
      }

      .content-tabs-container::-webkit-scrollbar {
        height: 4px;
      }

      .content-tabs-container::-webkit-scrollbar-thumb {
        background: rgba(0, 212, 255, 0.3);
        border-radius: 2px;
      }

      .content-tabs-container::-webkit-scrollbar-track {
        background: rgba(0, 10, 15, 0.5);
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
        /* Active tab connects to content below */
        background: rgba(15, 20, 25, 0.95);
        border-color: rgba(0, 212, 255, 0.4);
        border-bottom-color: transparent;
        color: #00d4ff;
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

    const template = `
      <div class="content-tabs-container">
        ${this.getTabsHTML()}
      </div>
    `;

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      ${template}
    `;

    // Add event listeners after DOM is created
    this.setupEventListeners();

    console.log('✅ ContentTabsWidget: Rendered with', this.tabs.length, 'tabs');
  }

  /**
   * Generate tabs HTML
   */
  private getTabsHTML(): string {
    if (this.tabs.length === 0) {
      return '<div class="empty-state">No content tabs</div>';
    }

    return this.tabs.map(tab => `
      <div
        class="content-tab ${tab.active ? 'active' : ''}"
        data-tab-id="${tab.id}"
      >
        <span class="tab-label">${tab.label}</span>
        ${tab.closeable ? '<span class="tab-close" data-close-tab="${tab.id}">×</span>' : ''}
      </div>
    `).join('');
  }

  /**
   * Setup event listeners for tabs
   */
  private setupEventListeners(): void {
    // Tab click handlers
    this.shadowRoot?.querySelectorAll('.content-tab').forEach(tab => {
      tab.addEventListener('click', (event) => {
        const target = event.target as HTMLElement;

        // Check if close button was clicked
        if (target.classList.contains('tab-close') || target.hasAttribute('data-close-tab')) {
          const tabId = target.getAttribute('data-close-tab') ||
                       (target.parentElement as HTMLElement)?.dataset.tabId;
          if (tabId) {
            this.handleTabClose(tabId);
          }
          event.stopPropagation();
          return;
        }

        // Handle tab selection
        const tabId = (tab as HTMLElement).dataset.tabId;
        if (tabId) {
          this.handleTabClick(tabId);
        }
      });
    });
  }

  /**
   * Handle tab click
   */
  private handleTabClick(tabId: string): void {
    console.log('📋 ContentTabsWidget: Tab clicked:', tabId);

    // Emit event for parent widget to handle navigation
    Events.emit('tabs:clicked', { tabId });

    // Also dispatch DOM event
    this.dispatchEvent(new CustomEvent('tab-clicked', {
      bubbles: true,
      composed: true,
      detail: { tabId }
    }));
  }

  /**
   * Handle tab close
   */
  private handleTabClose(tabId: string): void {
    console.log('📋 ContentTabsWidget: Tab close requested:', tabId);

    // Emit event for parent widget to handle close logic
    Events.emit('tabs:close', { tabId });

    // Also dispatch DOM event
    this.dispatchEvent(new CustomEvent('tab-closed', {
      bubbles: true,
      composed: true,
      detail: { tabId }
    }));
  }

  /**
   * Public API: Update tabs from parent widget
   */
  public updateTabs(tabs: TabInfo[]): void {
    this.tabs = tabs;
    this.renderWidget();
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🧹 ContentTabsWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
