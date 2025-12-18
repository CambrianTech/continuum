/**
 * MainPanel - Main Content Area Widget
 * 
 * Contains content header with tabs, version info, status buttons,
 * and the main content view that displays different page widgets.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { ContentInfoManager, ContentInfo } from './shared/ContentTypes';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { COMMANDS } from '../../shared/generated-command-constants';

export class MainWidget extends BaseWidget {
  private currentPath = '/chat/general'; // Current open room/path
  private contentManager: ContentInfoManager;
  private currentContent: ContentInfo | null = null;
  
  constructor() {
    super({
      widgetName: 'MainWidget',
      template: 'main-panel.html',
      styles: 'main-panel.css',
      enableAI: false,
      enableDatabase: true, // Enable database for room/content management
      enableRouterEvents: true,
      enableScreenshots: false
    });
    
    // Initialize content manager with widget context
    this.contentManager = new ContentInfoManager(this);
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎯 MainPanel: Initializing main content panel...');

    // Initialize content tabs
    await this.initializeContentTabs();

    // Listen to header controls events
    this.setupHeaderControlsListeners();

    // Listen for content:opened events to refresh tabs
    this.subscribeToContentEvents();

    // PHASE 3BIS: Track tab visibility for temperature
    this.setupVisibilityTracking();

    console.log('✅ MainPanel: Main panel initialized');
  }

  protected async renderWidget(): Promise<void> {
    // Use BaseWidget's template and styles system
    const styles = this.templateCSS || '/* No styles loaded */';
    const template = this.templateHTML || '<div>No template loaded</div>';

    // Ensure template is a string
    const templateString = typeof template === 'string' ? template : '<div>Template error</div>';

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      ${templateString}
    `;

    // Add event listeners after DOM is created
    this.setupEventListeners();

    // Update content tabs widget with current tab data
    await this.updateContentTabs();

    console.log('✅ MainPanel: Main panel rendered');
  }

  private setupEventListeners(): void {
    // Listen to tab events from content-tabs-widget
    this.addEventListener('tab-clicked', ((event: CustomEvent) => {
      const tabId = event.detail.tabId;
      console.log('📋 MainPanel: Tab clicked event received:', tabId);
      // Future: Handle tab navigation/switching
    }) as EventListener);

    this.addEventListener('tab-closed', ((event: CustomEvent) => {
      const tabId = event.detail.tabId;
      console.log('📋 MainPanel: Tab close event received:', tabId);
      // Future: Handle tab closing
    }) as EventListener);
  }

  private openThemeTab(): void {
    // Show theme modal/overlay instead of switching tabs
    this.showThemeModal();
  }

  private switchToTab(tabName: string): void {
    // Remove active class from all tabs
    this.shadowRoot?.querySelectorAll('.content-tab').forEach(tab => {
      tab.classList.remove('active');
    });

    // Add active class to selected tab
    const selectedTab = this.shadowRoot?.querySelector(`[data-tab="${tabName}"]`);
    selectedTab?.classList.add('active');

    // Update content view based on tab
    this.updateContentView(tabName);
    
    console.log(`📄 MainPanel: Switched to tab: ${tabName}`);
  }

  private updateContentView(tabName: string): void {
    const contentView = this.shadowRoot?.querySelector('.content-view');
    if (!contentView) return;

    // Content view always shows chat for now
    // Theme is handled via modal, not content switching
    contentView.innerHTML = '<chat-widget></chat-widget>';
  }

  private showThemeModal(): void {
    // Remove existing dropdown
    const existingDropdown = this.shadowRoot?.querySelector('.theme-slide-panel');
    if (existingDropdown) {
      existingDropdown.remove();
      return; // Toggle behavior - close if already open
    }

    // Create slide-down panel from the top
    const slidePanel = document.createElement('div');
    slidePanel.className = 'theme-slide-panel';
    slidePanel.innerHTML = `
      <theme-widget></theme-widget>
    `;

    // Add fast smooth slide-down styles
    const style = document.createElement('style');
    style.textContent = `
      .theme-slide-panel {
        position: fixed;
        top: 0;
        right: 20px;
        z-index: 2000;
        width: 320px;
        background: rgba(0, 10, 15, 0.98);
        border: 2px solid #00d4ff;
        border-top: none;
        border-radius: 0 0 8px 8px;
        box-shadow: 
          0 8px 24px rgba(0, 212, 255, 0.4),
          inset 0 1px 0 rgba(0, 212, 255, 0.2);
        overflow: hidden;
        
        /* Fast smooth slide from top */
        animation: slide-down-fast 0.2s cubic-bezier(0.23, 1, 0.32, 1);
        transform-origin: top center;
      }
      
      @keyframes slide-down-fast {
        0% {
          transform: translateY(-100%);
          opacity: 0;
        }
        100% {
          transform: translateY(0);
          opacity: 1;
        }
      }
      
      /* Cyberpunk border glow */
      .theme-slide-panel::before {
        content: '';
        position: absolute;
        top: 0;
        left: -2px;
        right: -2px;
        bottom: -2px;
        background: linear-gradient(180deg, #00d4ff, rgba(0, 212, 255, 0.1));
        border-radius: 0 0 8px 8px;
        z-index: -1;
        opacity: 0.6;
      }
    `;

    // Prevent panel clicks from propagating (stops click-outside detection)
    slidePanel.addEventListener('click', (event) => {
      const target = event.target as HTMLElement;
      
      // Handle Apply button - close panel after applying
      if (target.id === 'apply-theme') {
        setTimeout(() => {
          slidePanel.remove();
          style.remove();
          document.removeEventListener('click', handleClickOutside);
        }, 200);
      } else {
        // For all other clicks inside panel, stop propagation to prevent closing
        event.stopPropagation();
      }
    });

    // Append to shadow root for proper positioning
    this.shadowRoot?.appendChild(style);
    this.shadowRoot?.appendChild(slidePanel);

    // Close panel function
    const closePanel = () => {
      slidePanel.remove();
      style.remove();
      document.removeEventListener('click', handleClickOutside);
    };

    // Click-outside detection
    const handleClickOutside = (event: Event) => {
      closePanel();
    };

    // Add click-outside detection with delay
    setTimeout(() => {
      document.addEventListener('click', handleClickOutside);
    }, 200);

    // Listen for theme-cancelled event from Cancel button
    slidePanel.addEventListener('theme-cancelled', closePanel);
    
    // Listen for theme-applied event from Apply button
    slidePanel.addEventListener('theme-applied', (event) => {
      console.log('🎨 MainPanel: Theme applied, closing panel');
      closePanel();
    });

    console.log('🎨 MainPanel: Theme slide panel opened');
  }

  /**
   * Navigate to a different path (e.g., /chat/academy, /chat/user-123)
   */
  async navigateToPath(newPath: string): Promise<void> {
    const [, pathType, roomId] = newPath.split('/');
    
    // Check if room exists, create if needed (especially for user chats)
    if (pathType === 'chat' && roomId) {
      await this.ensureRoomExists(roomId);
    }
    
    // Update current path and clear cached content
    this.currentPath = newPath;
    this.currentContent = null;
    
    // Load new content information
    await this.loadCurrentContent();
    
    // Subscribe to events for this path
    await this.subscribeToPathEvents(newPath);
    
    // Re-render to update tabs and content for new path
    await this.renderWidget();
    
    const contentName = this.currentContent ? (this.currentContent as any).displayName : 'unknown';
    console.log(`🔄 MainPanel: Navigated to path: ${newPath} (${contentName})`);
  }

  /**
   * Ensure room exists using proper data classes and database daemon
   */
  private async ensureRoomExists(roomId: string): Promise<void> {
    try {
      // Use content manager to get or create room
      const content = await this.contentManager.getContentByPath(`/chat/${roomId}`);
      
      if (!content) {
        // Determine room type based on roomId
        const roomType = roomId.startsWith('user-') ? 'user_chat' : 'private';
        await this.contentManager.createRoom(roomId, roomType);
        console.log(`🏠 MainPanel: Created new room: ${roomId} (${roomType})`);
      } else {
        console.log(`✅ MainPanel: Room exists: ${roomId} (${content.displayName})`);
      }
    } catch (error) {
      console.error(`❌ MainPanel: Failed to ensure room ${roomId} exists:`, error);
    }
  }

  /**
   * Subscribe to events for the current path
   */
  private async subscribeToPathEvents(path: string): Promise<void> {
    // Events already have path built in, so just subscribe to this path
    console.log(`📡 MainPanel: Subscribing to events for path: ${path}`);
    // The actual event subscription would happen here
    // For now, just log that we're subscribing to path-based events
  }

  /**
   * PHASE 3BIS: Track tab visibility for temperature
   */
  private setupVisibilityTracking(): void {
    document.addEventListener('visibilitychange', async () => {
      // Extract roomId from currentPath (/chat/general → general)
      const [, pathType, roomId] = this.currentPath.split('/');

      if (pathType === 'chat' && roomId) {
        const present = !document.hidden;

        try {
          await Commands.execute(COMMANDS.COLLABORATION_ACTIVITY_USER_PRESENT, {
            activityId: roomId,
            present
          } as any);  // Cast to any for new command not yet in type registry
          console.log(`🌡️ MainPanel: User ${present ? 'present' : 'left'} in room ${roomId}`);
        } catch (error) {
          // Silently ignore when disconnected - this is expected
          const isDisconnected = error instanceof Error &&
            (error.message.includes('WebSocket not ready') || error.message.includes('WebSocket not connected'));
          if (!isDisconnected) {
            console.error('❌ MainPanel: Failed to track visibility:', error);
          }
        }
      }
    });

    console.log('👁️ MainPanel: Visibility tracking initialized');
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🧹 MainPanel: Cleanup complete');
  }

  /**
   * Initialize content tabs system
   */
  private async initializeContentTabs(): Promise<void> {
    // Set up tab switching logic
    console.log('📋 MainPanel: Content tabs initialized');
  }

  /**
   * Setup header controls event listeners
   */
  private setupHeaderControlsListeners(): void {
    // Listen to theme-clicked event from header-controls-widget
    this.addEventListener('theme-clicked', () => {
      this.openThemeTab();
    });

    // Listen to settings-clicked event
    this.addEventListener('settings-clicked', () => {
      console.log('⚙️ MainPanel: Settings button clicked from header controls');
      // Future: Open settings panel
    });

    // Listen to help-clicked event
    this.addEventListener('help-clicked', () => {
      console.log('❓ MainPanel: Help button clicked from header controls');
      // Future: Open help panel
    });

    console.log('🔗 MainPanel: Header controls listeners registered');
  }

  /**
   * Subscribe to content events (opened, closed, switched)
   */
  private subscribeToContentEvents(): void {
    // Listen for content:opened events from content/open command
    Events.subscribe('content:opened', async () => {
      console.log('📋 MainPanel: Received content:opened event, refreshing tabs');

      // Reload userState from database to get fresh openItems
      await this.loadUserContext();

      // Refresh tabs display with new data
      await this.updateContentTabs();

      console.log('✅ MainPanel: Tabs refreshed after content opened');
    });

    console.log('🔗 MainPanel: Subscribed to content events');
  }

  /**
   * Load content information for current path
   */
  private async loadCurrentContent(): Promise<void> {
    try {
      this.currentContent = await this.contentManager.getContentByPath(this.currentPath);
      console.log(`📄 MainPanel: Loaded content info for ${this.currentPath}:`, this.currentContent?.displayName);
    } catch (error) {
      console.error(`❌ MainPanel: Failed to load content for ${this.currentPath}:`, error);
      this.currentContent = null;
    }
  }

  /**
   * Update content tabs widget with current tab data
   */
  private async updateContentTabs(): Promise<void> {
    const tabsWidget = this.shadowRoot?.querySelector('content-tabs-widget');
    if (!tabsWidget) {
      console.warn('⚠️ MainPanel: ContentTabsWidget not found in shadow root');
      return;
    }

    // Read tabs from userState.contentState.openItems
    const tabs = [];

    if (this.userState?.contentState?.openItems) {
      // Map content items to tab format
      for (const item of this.userState.contentState.openItems) {
        tabs.push({
          id: item.id,
          label: item.title,
          active: item.id === this.userState.contentState.currentItemId,
          closeable: true
        });
      }
    } else {
      // Fallback: show current room as single tab if userState not loaded yet
      if (!this.currentContent) {
        await this.loadCurrentContent();
      }

      if (this.currentContent) {
        const displayName = this.currentContent.displayName || this.currentContent.name;
        tabs.push({
          id: this.currentContent.id,
          label: displayName,
          active: true,
          closeable: false
        });
      } else {
        // Final fallback for loading states
        const [, pathType, roomId] = this.currentPath.split('/');
        const fallbackName = roomId ? roomId.charAt(0).toUpperCase() + roomId.slice(1) : 'Chat';

        tabs.push({
          id: roomId || 'default',
          label: fallbackName,
          active: true,
          closeable: false
        });
      }
    }

    // Call updateTabs method on the widget
    (tabsWidget as any).updateTabs(tabs);

    console.log('📋 MainPanel: Updated content tabs:', tabs.length, 'tabs from', this.userState?.contentState ? 'userState' : 'fallback');
  }

  /**
   * Switch to a different content page
   */
  switchToPage(pageName: string): void {
    console.log(`📄 MainPanel: Switching to page: ${pageName}`);
    // Will update the content view to show different widgets
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry