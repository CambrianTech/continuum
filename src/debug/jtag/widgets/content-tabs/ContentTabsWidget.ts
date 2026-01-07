/**
 * ContentTabsWidget - Dynamic content tabs for navigation
 *
 * Displays content tabs based on current context (rooms, threads, pages).
 * Emits tab-clicked events for parent widget to handle navigation.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';
import { Commands } from '../../system/core/shared/Commands';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

export interface TabInfo {
  id: string;
  label: string;
  active: boolean;
  closeable?: boolean;
  /** Entity ID (e.g., room UUID for chat content) */
  entityId?: string;
  /** Content type (e.g., 'chat', 'settings') */
  contentType?: string;
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
    verbose() && console.log('📋 ContentTabsWidget: Initializing content tabs...');

    // Subscribe to tab updates from parent or router
    Events.subscribe('tabs:update', (tabs: TabInfo[]) => {
      this.tabs = tabs;
      this.renderWidget();
    });

    verbose() && console.log('✅ ContentTabsWidget: Initialized');
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
        /* Hide scrollbar but keep scrollable */
        scrollbar-width: none;  /* Firefox */
        -ms-overflow-style: none;  /* IE/Edge */
      }

      .content-tabs-container::-webkit-scrollbar {
        display: none;  /* Chrome/Safari - hide scrollbar */
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

    verbose() && console.log('✅ ContentTabsWidget: Rendered with', this.tabs.length, 'tabs');
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
   * Setup event listeners for tabs - direct listeners on each element
   */
  private setupEventListeners(): void {
    verbose() && console.log('📋 ContentTabsWidget.setupEventListeners: Setting up direct click listeners');

    // Add click listener to each tab directly
    const tabs = this.shadowRoot?.querySelectorAll('.content-tab');
    if (!tabs || tabs.length === 0) {
      verbose() && console.warn('📋 ContentTabsWidget: No tabs found for event listeners');
      return;
    }

    tabs.forEach((tabElement) => {
      const tab = tabElement as HTMLElement;
      const tabId = tab.dataset.tabId;

      verbose() && console.log('📋 ContentTabsWidget: Adding click listener to tab:', tabId);

      tab.addEventListener('click', (event) => {
        const target = event.target as HTMLElement;
        verbose() && console.log('🔥 TAB CLICKED! tabId:', tabId, 'target:', target.tagName, target.className);

        // Check if close button was clicked
        if (target.classList.contains('tab-close') || target.hasAttribute('data-close-tab')) {
          if (tabId) {
            this.handleTabClose(tabId);
          }
          event.stopPropagation();
          return;
        }

        // Handle tab selection
        if (tabId) {
          this.handleTabClick(tabId);
        }
      });
    });

    verbose() && console.log('📋 ContentTabsWidget: Added listeners to', tabs.length, 'tabs');
  }

  /**
   * Handle tab click
   */
  private handleTabClick(tabId: string): void {
    verbose() && console.log('🔥 ContentTabsWidget.handleTabClick CALLED with tabId:', tabId);
    verbose() && console.log('🔥 ContentTabsWidget: this.tabs has', this.tabs.length, 'items:', this.tabs.map(t => ({id: t.id, label: t.label})));

    // Find the full tab data
    const tab = this.tabs.find(t => t.id === tabId);
    if (!tab) {
      verbose() && console.warn('📋 ContentTabsWidget: Tab not found:', tabId, '- available tabs:', this.tabs.map(t => t.id));
      return;
    }

    verbose() && console.log('📋 ContentTabsWidget: Tab clicked:', tabId, 'entityId:', tab.entityId, 'type:', tab.contentType);

    // Emit event with full tab data for parent widget
    const tabData = {
      tabId: tab.id,
      label: tab.label,
      entityId: tab.entityId,
      contentType: tab.contentType
    };

    // DEBUG: Call ping command to verify click is working (visible in server logs)
    Commands.execute('ping', {}).then(r => verbose() && console.log('🔥 TAB CLICK VERIFIED via ping:', r));

    Events.emit('tabs:clicked', tabData);

    // Also dispatch DOM event with full tab data
    this.dispatchEvent(new CustomEvent('tab-clicked', {
      bubbles: true,
      composed: true,
      detail: tabData
    }));
  }

  /**
   * Handle tab close
   */
  private handleTabClose(tabId: string): void {
    verbose() && console.log('📋 ContentTabsWidget: Tab close requested:', tabId);

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
    verbose() && console.log('🧹 ContentTabsWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
