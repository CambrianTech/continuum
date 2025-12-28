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
import { UI_EVENTS } from '../../system/core/shared/EventConstants';
import { COMMANDS } from '../../shared/generated-command-constants';
import type { StateContentCloseParams, StateContentCloseResult } from '../../commands/state/content/close/shared/StateContentCloseTypes';
import type { StateContentSwitchParams, StateContentSwitchResult } from '../../commands/state/content/switch/shared/StateContentSwitchTypes';
import type { ContentOpenParams, ContentOpenResult } from '../../commands/collaboration/content/open/shared/ContentOpenTypes';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { ContentType, ContentPriority, ContentItem } from '../../system/data/entities/UserStateEntity';
import { DEFAULT_ROOMS } from '../../system/data/domains/DefaultEntities';
import { getWidgetForType, buildContentPath, parseContentPath, getRightPanelConfig, initializeRecipeLayouts } from './shared/ContentTypeRegistry';
import { PositronContentStateAdapter } from '../shared/services/state/PositronContentStateAdapter';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { RoutingService } from '../../system/routing/RoutingService';
// Theme loading removed - handled by ContinuumWidget

export class MainWidget extends BaseWidget {
  private currentPath = '/chat/general'; // Current open room/path
  private contentManager: ContentInfoManager;
  private currentContent: ContentInfo | null = null;
  private contentStateAdapter: PositronContentStateAdapter;

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

    // Initialize Positron content state adapter
    // This handles content:opened/closed/switched events with proper state management
    this.contentStateAdapter = new PositronContentStateAdapter(
      () => this.userState,
      {
        name: 'MainWidget',
        onStateChange: () => this.updateContentTabs(),
        onViewSwitch: (contentType, entityId) => this.switchContentView(contentType, entityId),
        onUrlUpdate: (contentType, entityId) => {
          const newPath = buildContentPath(contentType, entityId);
          this.updateUrl(newPath);
        },
        onFallback: () => this.refreshTabsFromDatabase('fallback')
      }
    );
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🎯 MainPanel: Initializing main content panel...');

    // Load recipe layouts early so ContentTypeRegistry can use them
    // This enables dynamic, recipe-driven content type → widget mapping
    await initializeRecipeLayouts();

    // Theme CSS is loaded by ContinuumWidget (parent) in onWidgetInitialize
    // Don't load again here - it would remove base.css variables

    // Initialize content tabs
    await this.initializeContentTabs();

    // Listen to header controls events
    this.setupHeaderControlsListeners();

    // Listen for content:opened events to refresh tabs
    this.subscribeToContentEvents();

    // Setup URL routing (back/forward navigation)
    this.setupUrlRouting();

    // PHASE 3BIS: Track tab visibility for temperature
    this.setupVisibilityTracking();

    console.log('✅ MainPanel: Main panel initialized');
  }

  /**
   * Setup URL-based routing for bookmarks and back/forward
   */
  private setupUrlRouting(): void {
    // Handle browser back/forward
    window.addEventListener('popstate', (event) => {
      const path = event.state?.path || window.location.pathname;
      this.navigateToPath(path);
    });

    // Initialize from current URL
    const initialPath = window.location.pathname;
    if (initialPath && initialPath !== '/') {
      this.currentPath = initialPath;
      // Parse and load the content
      const { type, entityId } = parseContentPath(initialPath);
      console.log(`🔗 MainPanel: Initial route: ${type}/${entityId || 'default'}`);

      // Switch to the content view based on URL
      // Delay slightly to let the DOM render first
      setTimeout(async () => {
        this.switchContentView(type, entityId);

        // Ensure a tab exists for this URL (URL is source of truth for content)
        await this.ensureTabForContent(type, entityId);

        // For chat, resolve uniqueId → UUID and emit ROOM_SELECTED so ChatWidget loads the room
        if (type === 'chat' && entityId) {
          const resolved = await RoutingService.resolveRoom(entityId);
          if (resolved) {
            Events.emit(UI_EVENTS.ROOM_SELECTED, {
              roomId: resolved.id,
              roomName: resolved.displayName,
              uniqueId: resolved.uniqueId  // For URL building
            });
          } else {
            console.warn(`⚠️ MainPanel: Could not resolve room: ${entityId}`);
            // Fallback to using the identifier as-is (might be a UUID already)
            Events.emit(UI_EVENTS.ROOM_SELECTED, { roomId: entityId, roomName: '', uniqueId: entityId });
          }
        }
      }, 100);
    }
  }

  /**
   * Ensure a tab exists for the given content type and entityId
   * Creates tab if it doesn't exist, selects it if it does
   */
  private async ensureTabForContent(contentType: string, entityId?: string): Promise<void> {
    // Check if tab already exists
    const existingTab = this.userState?.contentState?.openItems?.find(
      item => item.type === contentType && item.entityId === entityId
    );

    if (existingTab) {
      // Tab exists - just make sure it's current
      if (this.userState?.contentState) {
        this.userState.contentState.currentItemId = existingTab.id;
      }
      this.updateContentTabs();
      return;
    }

    // Create tab via content/open command
    const userId = this.userState?.userId;
    if (userId) {
      // Get title from entityId or content type
      const title = entityId
        ? entityId.charAt(0).toUpperCase() + entityId.slice(1)
        : contentType.charAt(0).toUpperCase() + contentType.slice(1);

      try {
        await Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
          userId: userId as UUID,
          contentType: contentType as ContentType,
          entityId: entityId,
          title: title,
          setAsCurrent: true
        });
        // Refresh tabs from DB
        await this.loadUserContext();
        await this.updateContentTabs();
        console.log(`📋 MainPanel: Created tab for ${contentType}/${entityId || 'default'}`);
      } catch (err) {
        console.error(`Failed to create tab for ${contentType}:`, err);
      }
    }
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
    // Tab click data includes entityId and contentType so we don't need userState lookup
    type TabClickData = {
      tabId: string;
      label?: string;
      entityId?: string;
      contentType?: string;
    };

    // Listen to tab events from content-tabs-widget via DOM events
    this.addEventListener('tab-clicked', ((event: CustomEvent) => {
      const tabData: TabClickData = event.detail;
      console.log('📋 MainWidget: Received tab-clicked DOM event:', tabData.tabId);
      this.handleTabClick(tabData);
    }) as EventListener);

    this.addEventListener('tab-closed', ((event: CustomEvent) => {
      const tabId = event.detail.tabId;
      this.handleTabClose(tabId);
    }) as EventListener);

    // Also listen via Events system (more reliable across shadow DOM)
    Events.subscribe('tabs:clicked', (data: TabClickData) => {
      console.log('📋 MainWidget: Received tabs:clicked event:', data.tabId);
      this.handleTabClick(data);
    });

    Events.subscribe('tabs:close', (data: { tabId: string }) => {
      this.handleTabClose(data.tabId);
    });
  }

  /**
   * Handle tab click - switch to that content
   * Uses tab data passed from ContentTabsWidget (no userState lookup needed)
   */
  private handleTabClick(tabData: { tabId: string; label?: string; entityId?: string; contentType?: string }): void {
    console.log('🔥 MainWidget.handleTabClick CALLED with:', tabData);

    const { tabId, label, entityId, contentType } = tabData;

    console.log(`📋 MainPanel.handleTabClick: tabId="${tabId}", entityId="${entityId}", type="${contentType}"`);

    // If missing entityId, try to look up from userState as fallback
    let resolvedEntityId = entityId;
    let resolvedContentType = contentType;

    if (!resolvedEntityId || !resolvedContentType) {
      console.warn(`⚠️ MainPanel: Tab missing entityId/contentType, looking up in userState...`);
      const contentItem = this.userState?.contentState?.openItems?.find(item => item.id === tabId);
      if (contentItem) {
        resolvedEntityId = contentItem.entityId;
        resolvedContentType = contentItem.type;
        console.log(`📋 MainPanel: Found in userState - entityId="${resolvedEntityId}", type="${resolvedContentType}"`);
      } else {
        console.error(`❌ MainPanel: Tab not found in userState either:`, tabId);
        return;
      }
    }

    // Already the current tab? Skip
    if (this.userState?.contentState?.currentItemId === tabId) {
      console.log('📋 MainPanel: Tab already current, skipping');
      return;
    }

    // IMPORTANT: Persist to database FIRST, before any events that might refresh from DB
    const userId = this.userState?.userId;
    if (userId) {
      // Fire and forget - but do it BEFORE emitting events
      Commands.execute<StateContentSwitchParams, StateContentSwitchResult>('state/content/switch', {
        userId: userId as UUID,
        contentItemId: tabId as UUID
      }).catch(err => console.error('Failed to persist tab switch:', err));
    }

    // Update local state immediately (optimistic UI)
    if (this.userState?.contentState) {
      this.userState.contentState.currentItemId = tabId;
    }

    // Update tab highlighting
    this.updateContentTabs();

    // Switch content view to the correct widget type
    this.switchContentView(resolvedContentType, resolvedEntityId);

    // Update URL (bookmarkable)
    const newPath = buildContentPath(resolvedContentType, resolvedEntityId);
    this.updateUrl(newPath);

    // Emit appropriate event based on content type
    if (resolvedContentType === 'chat' && resolvedEntityId) {
      // Resolve uniqueId → UUID for consistent room identification
      RoutingService.resolveRoom(resolvedEntityId).then(resolved => {
        if (resolved) {
          Events.emit(UI_EVENTS.ROOM_SELECTED, {
            roomId: resolved.id,
            roomName: resolved.displayName || label || 'Chat',
            uniqueId: resolved.uniqueId  // For URL building
          });
        } else {
          // Fallback to using the identifier as-is
          Events.emit(UI_EVENTS.ROOM_SELECTED, {
            roomId: resolvedEntityId,
            roomName: label || 'Chat',
            uniqueId: resolvedEntityId
          });
        }
      });
    }

    console.log(`📋 MainPanel: Switched to ${resolvedContentType} tab "${label}"`);
  }

  /**
   * Switch content view to render the appropriate widget
   * NOTE: Does NOT emit ROOM_SELECTED - caller is responsible for that to avoid loops
   * Emits RIGHT_PANEL_CONFIGURE to update right panel based on content type's layout
   */
  private switchContentView(contentType: string, entityId?: string): void {
    const contentView = this.shadowRoot?.querySelector('.content-view');
    if (!contentView) return;

    const widgetTag = getWidgetForType(contentType);

    // Create widget element with entity context if needed
    // Pass entityId as a data attribute so widgets can access it
    let widgetHtml = entityId
      ? `<${widgetTag} data-entity-id="${entityId}"></${widgetTag}>`
      : `<${widgetTag}></${widgetTag}>`;

    contentView.innerHTML = widgetHtml;

    // Emit right panel configuration based on content type's layout
    const rightPanelConfig = getRightPanelConfig(contentType);
    Events.emit(UI_EVENTS.RIGHT_PANEL_CONFIGURE, {
      widget: rightPanelConfig?.widget || null,
      room: rightPanelConfig?.room,
      compact: rightPanelConfig?.compact,
      contentType: contentType
    });

    // Emit Positron widget state for AI awareness
    PositronWidgetState.emit({
      widgetType: contentType,
      entityId: entityId,
      title: entityId ? `${contentType} - ${entityId}` : contentType,
      metadata: {
        widget: widgetTag,
        rightPanelRoom: rightPanelConfig?.room
      }
    });

    console.log(`🔄 MainPanel: Rendered ${widgetTag} for ${contentType}${entityId ? ` (${entityId})` : ''}, rightPanel: ${rightPanelConfig ? rightPanelConfig.room : 'hidden'}`);
  }

  /**
   * Update browser URL without full page reload
   */
  private updateUrl(path: string): void {
    if (this.currentPath !== path) {
      this.currentPath = path;
      window.history.pushState({ path }, '', path);
    }
  }

  /**
   * Handle tab close - remove from openItems
   */
  private async handleTabClose(tabId: string): Promise<void> {
    const contentItem = this.userState?.contentState?.openItems?.find(item => item.id === tabId);
    if (!contentItem) return;

    // Remove from local state immediately (optimistic UI)
    if (this.userState?.contentState) {
      this.userState.contentState.openItems = this.userState.contentState.openItems.filter(item => item.id !== tabId);

      // If we closed the current tab, switch to the first remaining tab
      if (this.userState.contentState.currentItemId === tabId) {
        const firstItem = this.userState.contentState.openItems[0];
        if (firstItem) {
          this.userState.contentState.currentItemId = firstItem.id;
          // Switch to the correct content view based on content type
          this.switchContentView(firstItem.type, firstItem.entityId);
          // Update URL
          const newPath = buildContentPath(firstItem.type, firstItem.entityId);
          this.updateUrl(newPath);
          // Only emit ROOM_SELECTED for chat content
          if (firstItem.type === 'chat' && firstItem.entityId) {
            // Resolve uniqueId → UUID for consistent room identification
            RoutingService.resolveRoom(firstItem.entityId).then(resolved => {
              if (resolved) {
                Events.emit(UI_EVENTS.ROOM_SELECTED, {
                  roomId: resolved.id,
                  roomName: resolved.displayName || firstItem.title,
                  uniqueId: resolved.uniqueId  // For URL building
                });
              } else {
                Events.emit(UI_EVENTS.ROOM_SELECTED, {
                  roomId: firstItem.entityId,
                  roomName: firstItem.title,
                  uniqueId: firstItem.entityId
                });
              }
            });
          }
        } else {
          // No tabs left - clear content area (like IDE with no files open)
          this.userState.contentState.currentItemId = undefined;
          this.currentPath = '/';
          this.updateUrl('/');
          const contentView = this.shadowRoot?.querySelector('.content-view');
          if (contentView) {
            contentView.innerHTML = '';
          }
        }
      }
    }

    // Update tabs immediately
    this.updateContentTabs();

    // Persist to server in background (don't await)
    const userId = this.userState?.userId;
    if (userId) {
      Commands.execute<StateContentCloseParams, StateContentCloseResult>('state/content/close', {
        userId: userId as UUID,
        contentItemId: tabId as UUID
      }).catch(err => console.error('Failed to persist tab close:', err));
    }

    console.log(`📋 MainPanel: Closed tab "${contentItem.title}"`);
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
    // Listen to theme-clicked event - opens Theme tab with integrated AI chat
    this.addEventListener('theme-clicked', () => {
      console.log('🎨 MainPanel: Theme button clicked - opening Theme tab');
      // Opens theme-widget with embedded chat for AI assistance
      // The Theme room (DEFAULT_ROOMS.THEME) provides the chat backend
      this.openContentTab('theme', 'Theme');
    });

    // Listen to settings-clicked event - opens Settings tab with integrated AI chat
    this.addEventListener('settings-clicked', () => {
      console.log('⚙️ MainPanel: Settings button clicked - opening Settings tab');
      // Opens settings-widget with embedded chat for AI assistance
      // The Settings room (DEFAULT_ROOMS.SETTINGS) provides the chat backend
      this.openContentTab('settings', 'Settings');
    });

    // Listen to help-clicked event - opens Help tab with integrated AI chat
    this.addEventListener('help-clicked', () => {
      console.log('❓ MainPanel: Help button clicked - opening Help tab');
      // Opens help-widget with embedded chat for AI assistance
      // The Help room (DEFAULT_ROOMS.HELP) provides the chat backend
      this.openContentTab('help', 'Help');
    });

    console.log('🔗 MainPanel: Header controls listeners registered');
  }

  /**
   * Subscribe to content events (opened, closed, switched) and room selection
   */
  private subscribeToContentEvents(): void {
    // Use Positron adapter for content:opened/closed/switched events
    // This delegates to PositronContentStateAdapter which updates local state
    // directly from event data instead of refetching from DB
    this.contentStateAdapter.subscribeToEvents();

    // IMPORTANT: Also listen for ROOM_SELECTED as reliable backup
    // RoomListWidget emits this and it definitely works (sidebar highlights change)
    Events.subscribe(UI_EVENTS.ROOM_SELECTED, (data: { roomId: string; roomName: string; uniqueId?: string }) => {
      console.log('📋 MainPanel: Received ROOM_SELECTED event:', data.roomName);

      // Only switch to chat if we're currently viewing chat content
      // Don't override settings/help/theme when sidebar highlights a room
      const { type: currentType } = parseContentPath(this.currentPath);
      if (currentType !== 'chat') {
        console.log(`📋 MainPanel: Ignoring ROOM_SELECTED - currently on ${currentType}, not chat`);
        return;
      }

      // Update URL for room selection (bookmarkable deep links)
      // Use uniqueId for human-readable URLs, fall back to roomId if not available
      const urlIdentifier = data.uniqueId || data.roomId;
      const newPath = buildContentPath('chat', urlIdentifier);
      this.updateUrl(newPath);

      // Switch to the selected chat room (use uniqueId for content view)
      this.switchContentView('chat', urlIdentifier);

      // Small delay to let the content/open command complete first
      setTimeout(() => this.refreshTabsFromDatabase('ROOM_SELECTED'), 100);
    });

    console.log('🔗 MainPanel: Subscribed to content events and ROOM_SELECTED');
  }

  /**
   * Refresh tabs by fetching from database
   * Used as fallback when Positron local state isn't available
   */
  private async refreshTabsFromDatabase(source: string): Promise<void> {
    try {
      console.log(`📋 MainPanel: Refreshing tabs from DB (${source})...`);
      await this.loadUserContext();
      await this.updateContentTabs();
      console.log(`✅ MainPanel: Tabs refreshed from DB (${source}), now ${this.userState?.contentState?.openItems?.length} items`);
    } catch (error) {
      console.error(`❌ MainPanel: Error refreshing tabs from DB (${source}):`, error);
    }
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
      // Map content items to tab format with full data for click handling
      for (const item of this.userState.contentState.openItems) {
        const tabData = {
          id: item.id,
          label: item.title,
          active: item.id === this.userState.contentState.currentItemId,
          closeable: true,
          entityId: item.entityId,
          contentType: item.type
        };
        console.log('📋 MainWidget.updateContentTabs: Creating tab:', tabData.label, 'entityId:', tabData.entityId, 'type:', tabData.contentType);
        tabs.push(tabData);
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

  /**
   * Open a content tab (e.g., settings, help) or switch to it if already open
   */
  private openContentTab(contentType: string, title: string): void {
    // Check if tab already exists
    const existingTab = this.userState?.contentState?.openItems?.find(
      item => item.type === contentType
    );

    if (existingTab) {
      // Tab exists - just switch to it with full data
      this.handleTabClick({
        tabId: existingTab.id,
        label: existingTab.title,
        entityId: existingTab.entityId,
        contentType: existingTab.type
      });
      return;
    }

    // Create new tab in local state with all required properties
    const newTabId = `${contentType}-${Date.now()}` as UUID;
    const newTab = {
      id: newTabId,
      type: contentType as ContentType,
      title: title,
      lastAccessedAt: new Date(),
      priority: 'normal' as ContentPriority
    };

    // Add to openItems (optimistic UI)
    if (this.userState?.contentState) {
      this.userState.contentState.openItems.push(newTab);
      this.userState.contentState.currentItemId = newTabId;
    }

    // Update tabs UI
    this.updateContentTabs();

    // Switch to the new content view
    this.switchContentView(contentType);

    // Update URL
    const newPath = buildContentPath(contentType);
    this.updateUrl(newPath);

    // PERSIST to database - singletons like settings have no entityId
    const userId = this.userState?.userId;
    if (userId) {
      Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
        userId: userId as UUID,
        contentType: contentType as ContentType,
        title: title,
        setAsCurrent: true
      }).catch(err => console.error(`Failed to persist ${contentType} tab:`, err));
    }

    console.log(`📋 MainPanel: Opened new ${contentType} tab`);
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry