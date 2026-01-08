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
import { ROOM_UNIQUE_IDS } from '../../system/data/constants/RoomConstants';
import { getWidgetForType, buildContentPath, parseContentPath, getRightPanelConfig, initializeRecipeLayouts } from './shared/ContentTypeRegistry';
import { PositronContentStateAdapter } from '../shared/services/state/PositronContentStateAdapter';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { RoutingService } from '../../system/routing/RoutingService';
import { pageState } from '../../system/state/PageStateService';
import { contentState } from '../../system/state/ContentStateService';
// Theme loading removed - handled by ContinuumWidget

export class MainWidget extends BaseWidget {
  private currentPath = `/chat/${ROOM_UNIQUE_IDS.GENERAL}`; // Current open room/path
  private contentManager: ContentInfoManager;
  private currentContent: ContentInfo | null = null;
  private contentStateAdapter: PositronContentStateAdapter;

  // Guard against infinite re-render loops
  private currentViewType: string | null = null;
  private currentViewEntityId: string | undefined = undefined;

  // Widget cache - persist widgets instead of destroying them on tab switch
  // Key: widget tag name (e.g., 'chat-widget', 'settings-widget')
  // Value: the cached widget element
  private widgetCache = new Map<string, HTMLElement>();

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
        onStateChange: () => this.syncUserStateToContentState(),
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
    this.verbose() && console.log('🎯 MainPanel: Initializing main content panel...');

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

    this.verbose() && console.log('✅ MainPanel: Main panel initialized');
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
    let initialPath = window.location.pathname;

    // Default route: / or /chat without room → /chat/general
    const defaultPath = `/chat/${ROOM_UNIQUE_IDS.GENERAL}`;
    if (!initialPath || initialPath === '/' || initialPath === '/chat' || initialPath === '/chat/') {
      initialPath = defaultPath;
      // Update URL without triggering navigation
      window.history.replaceState({ path: initialPath }, '', initialPath);
      this.verbose() && console.log(`🔗 MainPanel: Redirected to default route: ${initialPath}`);
    }

    this.currentPath = initialPath;
    // Parse and load the content
    const { type, entityId } = parseContentPath(initialPath);
    this.verbose() && console.log(`🔗 MainPanel: Initial route: ${type}/${entityId || 'default'}`);

    // URL → PageState → Widget (in that order)
    // Delay slightly to let the DOM render first
    setTimeout(async () => {
      // 1. Resolve entity for proper display name AND canonical UUID
      const resolved = entityId
        ? await RoutingService.resolve(type, entityId)
        : undefined;

      // Use resolved UUID for storage, uniqueId for URLs
      // This prevents duplicate tabs from UUID vs uniqueId mismatch
      const canonicalEntityId = resolved?.id || entityId;

      // 2. Set page state FIRST (single source of truth for widgets)
      pageState.setContent(type, canonicalEntityId, resolved || undefined);

      // 3. Ensure tab exists in UserState (use canonical UUID, not URL string)
      await this.ensureTabForContent(type, canonicalEntityId);

      // 4. THEN create widget (reads from pageState)
      this.switchContentView(type, canonicalEntityId);
    }, 100);
  }

  /**
   * Ensure a tab exists for the given content type and entityId
   * Creates tab if it doesn't exist, selects it if it does
   *
   * NOTE: Follows React pattern - ONLY call command, let event handle UI.
   */
  private async ensureTabForContent(contentType: string, entityId?: string): Promise<void> {
    // Check if tab already exists
    const existingTab = this.userState?.contentState?.openItems?.find(
      item => item.type === contentType && item.entityId === entityId
    );

    if (existingTab) {
      // Tab exists - switch to it via command
      const userId = this.userState?.userId;
      if (userId) {
        Commands.execute<StateContentSwitchParams, StateContentSwitchResult>('state/content/switch', {
          userId: userId as UUID,
          contentItemId: existingTab.id as UUID
        }).catch(err => console.error('Failed to switch to existing tab:', err));
      }
      return;
    }

    // Create tab via content/open command
    // Command emits content:opened, adapter handles state + UI
    const userId = this.userState?.userId;
    if (userId) {
      const title = entityId
        ? entityId.charAt(0).toUpperCase() + entityId.slice(1)
        : contentType.charAt(0).toUpperCase() + contentType.slice(1);

      Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
        userId: userId as UUID,
        contentType: contentType as ContentType,
        entityId: entityId,
        title: title,
        setAsCurrent: true
      }).catch(err => console.error(`Failed to create tab for ${contentType}:`, err));

      this.verbose() && console.log(`📋 MainPanel: Creating tab for ${contentType}/${entityId || 'default'} - command will handle state update`);
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

    // ContentTabsWidget subscribes to global contentState - no manual update needed

    this.verbose() && console.log('✅ MainPanel: Main panel rendered');
  }

  private setupEventListeners(): void {
    // Tab close handled via content:closed event → adapter → onStateChange
    // No DOM event listener needed - state drives UI
  }

  /**
   * Handle tab click - switch to that content
   * Uses tab data passed from ContentTabsWidget (no userState lookup needed)
   */
  private async handleTabClick(tabData: { tabId: string; label?: string; entityId?: string; contentType?: string }): Promise<void> {
    const { tabId, label, entityId, contentType } = tabData;

    // If missing entityId, try to look up from userState as fallback
    let resolvedEntityId = entityId;
    let resolvedContentType = contentType;

    if (!resolvedEntityId || !resolvedContentType) {
      console.warn(`⚠️ MainPanel: Tab missing entityId/contentType, looking up in userState...`);
      const contentItem = this.userState?.contentState?.openItems?.find(item => item.id === tabId);
      if (contentItem) {
        resolvedEntityId = contentItem.entityId;
        resolvedContentType = contentItem.type;
        this.verbose() && console.log(`📋 MainPanel: Found in userState - entityId="${resolvedEntityId}", type="${resolvedContentType}"`);
      } else {
        console.error(`❌ MainPanel: Tab not found in userState either:`, tabId);
        return;
      }
    }

    // Already the current tab? Skip
    if (this.userState?.contentState?.currentItemId === tabId) {
      this.verbose() && console.log('📋 MainPanel: Tab already current, skipping');
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

    // OPTIMISTIC UPDATE: Update global contentState immediately
    // The command will persist + emit event, but switchContentView's guard prevents duplicate render
    contentState.setCurrent(tabId as UUID);

    // Set pageState (ChatWidget subscribes to this)
    const resolved = resolvedEntityId
      ? await RoutingService.resolve(resolvedContentType, resolvedEntityId)
      : undefined;
    pageState.setContent(resolvedContentType, resolvedEntityId, resolved || undefined);

    // Instant UI switch (guard in switchContentView prevents duplicate if event arrives)
    this.switchContentView(resolvedContentType, resolvedEntityId);

    // Update URL
    const newPath = buildContentPath(resolvedContentType, resolvedEntityId);
    this.updateUrl(newPath);

    this.verbose() && console.log(`📋 MainPanel: Switched to ${resolvedContentType} tab "${label}"`);
  }

  /**
   * Switch content view to render the appropriate widget
   *
   * NEW ARCHITECTURE: Widget caching with hide/show instead of destroy/recreate
   * - Widgets are created once and cached
   * - Tab switching = hide old widget, show new widget (instant)
   * - State changes via direct method call, NOT attribute setting
   *
   * NOTE: pageState is set BEFORE calling this method - ChatWidget subscribes to pageState.
   * Emits RIGHT_PANEL_CONFIGURE to update right panel based on content type's layout
   */
  private switchContentView(contentType: string, entityId?: string): void {
    // GUARD: Prevent infinite re-render loops by checking if already showing this content
    if (this.currentViewType === contentType && this.currentViewEntityId === entityId) {
      return; // Already showing this exact content
    }

    const contentView = this.shadowRoot?.querySelector('.content-view') as HTMLElement;
    if (!contentView) return;

    const widgetTag = getWidgetForType(contentType);

    // Update tracking state
    this.currentViewType = contentType;
    this.currentViewEntityId = entityId;

    // === HIDE all cached widgets and notify them ===
    this.widgetCache.forEach((widget, tag) => {
      if (widget.style.display !== 'none') {
        widget.style.display = 'none';
        // Notify widget it's being deactivated (if it supports the method)
        if ('onDeactivate' in widget && typeof (widget as any).onDeactivate === 'function') {
          (widget as any).onDeactivate();
        }
        this.verbose() && console.log(`🎯 MainPanel: Deactivated ${tag}`);
      }
    });

    // === GET OR CREATE widget ===
    let widget = this.widgetCache.get(widgetTag);

    if (!widget) {
      // Check if widget already exists in DOM (from template)
      const existingInDom = contentView.querySelector(widgetTag) as HTMLElement;
      if (existingInDom) {
        // Use existing widget from template
        widget = existingInDom;
        this.widgetCache.set(widgetTag, widget);
        this.verbose() && console.log(`🎯 MainPanel: Cached existing ${widgetTag} from template`);
      } else {
        // First time seeing this widget type - create and cache it
        widget = document.createElement(widgetTag);
        widget.style.display = 'none'; // Start hidden
        contentView.appendChild(widget);
        this.widgetCache.set(widgetTag, widget);
        this.verbose() && console.log(`🎯 MainPanel: Created and cached ${widgetTag}`);
      }
    }

    // === ACTIVATE widget (show + notify) ===
    widget.style.display = '';

    // Notify widget of entity change via method call (NOT attribute)
    // This avoids triggering attributeChangedCallback which causes full re-renders
    if ('onActivate' in widget && typeof (widget as any).onActivate === 'function') {
      // New pattern: widgets implement onActivate(entityId) for state changes
      (widget as any).onActivate(entityId);
    } else if ('setEntityId' in widget && typeof (widget as any).setEntityId === 'function') {
      // Fallback: widgets implement setEntityId(entityId)
      (widget as any).setEntityId(entityId);
    } else {
      // Legacy fallback: set attribute (will trigger attributeChangedCallback)
      // TODO: Remove once all widgets implement onActivate()
      if (entityId) {
        widget.setAttribute('entity-id', entityId);
      } else {
        widget.removeAttribute('entity-id');
      }
    }

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

    this.verbose() && console.log(`🔄 MainPanel: Rendered ${widgetTag} for ${contentType}${entityId ? ` (${entityId})` : ''}, rightPanel: ${rightPanelConfig ? rightPanelConfig.room : 'hidden'}`);
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
   *
   * Uses optimistic update for instant UI, command persists in background.
   * Adapter's event handler will see state already updated, minimal duplicate work.
   */
  private async handleTabClose(tabId: string): Promise<void> {
    // Use global contentState
    const contentItem = contentState.openItems.find(item => item.id === tabId);
    if (!contentItem) return;

    const wasCurrentItem = contentState.currentItemId === tabId;

    // Remove from global state - ContentTabsWidget will re-render automatically
    contentState.removeItem(tabId as UUID);

    // If we closed the current tab, switch view
    if (wasCurrentItem) {
      const newCurrent = contentState.currentItem;
      if (newCurrent) {
        pageState.setContent(newCurrent.type, newCurrent.entityId, undefined);
        this.switchContentView(newCurrent.type, newCurrent.entityId);
        this.updateUrl(buildContentPath(newCurrent.type, newCurrent.entityId));
      } else {
        // No tabs left - open default
        const defaultRoom = ROOM_UNIQUE_IDS.GENERAL;
        pageState.setContent('chat', defaultRoom, undefined);
        this.switchContentView('chat', defaultRoom);
        this.updateUrl(`/chat/${defaultRoom}`);
      }
    }

    // Persist in background
    const userId = this.userState?.userId;
    if (userId) {
      Commands.execute<StateContentCloseParams, StateContentCloseResult>('state/content/close', {
        userId: userId as UUID,
        contentItemId: tabId as UUID
      }).catch(err => console.error('Failed to persist tab close:', err));
    }

    this.verbose() && console.log(`📋 MainPanel: Closed tab "${contentItem.title}"`);
  }

  // NOTE: Legacy switchToTab() and updateContentView() removed.
  // They used innerHTML which destroys child widgets.
  // switchContentView() is the correct implementation using widget cache.

  /**
   * Navigate to a different path (e.g., /chat/academy, /chat/user-123)
   */
  async navigateToPath(newPath: string): Promise<void> {
    const { type, entityId } = parseContentPath(newPath);

    // Check if room exists, create if needed (especially for user chats)
    if (type === 'chat' && entityId) {
      await this.ensureRoomExists(entityId);
    }

    // Update current path
    this.currentPath = newPath;

    // 1. Resolve entity for display name
    const resolved = entityId
      ? await RoutingService.resolve(type, entityId)
      : undefined;

    // 2. Set page state FIRST (single source of truth)
    pageState.setContent(type, entityId, resolved || undefined);

    // 3. Ensure a tab exists for this URL
    await this.ensureTabForContent(type, entityId);

    // 4. Switch content view (ChatWidget subscribes to pageState)
    this.switchContentView(type, entityId);

    this.verbose() && console.log(`🔄 MainPanel: Navigated to ${type}/${entityId || 'default'}`);
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
        this.verbose() && console.log(`🏠 MainPanel: Created new room: ${roomId} (${roomType})`);
      } else {
        this.verbose() && console.log(`✅ MainPanel: Room exists: ${roomId} (${content.displayName})`);
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
    this.verbose() && console.log(`📡 MainPanel: Subscribing to events for path: ${path}`);
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
          this.verbose() && console.log(`🌡️ MainPanel: User ${present ? 'present' : 'left'} in room ${roomId}`);
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

    this.verbose() && console.log('👁️ MainPanel: Visibility tracking initialized');
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.verbose() && console.log('🧹 MainPanel: Cleanup complete');
  }

  /**
   * Initialize content tabs system
   */
  private async initializeContentTabs(): Promise<void> {
    // Initialize global contentState from persisted userState
    if (this.userState?.contentState) {
      const openItems = this.userState.contentState.openItems || [];
      const currentItemId = this.userState.contentState.currentItemId;
      contentState.initialize(openItems, currentItemId);
      this.verbose() && console.log(`📋 MainPanel: Initialized global contentState with ${openItems.length} items`);
    } else {
      this.verbose() && console.log('📋 MainPanel: No persisted contentState, starting empty');
      contentState.initialize([], undefined);
    }
  }

  /**
   * Sync userState.contentState to global contentState
   * Called when PositronContentStateAdapter updates userState from server events
   */
  private syncUserStateToContentState(): void {
    if (!this.userState?.contentState) return;

    // Re-initialize contentState from userState
    // This syncs server-persisted changes to the global state
    const openItems = this.userState.contentState.openItems || [];
    const currentItemId = this.userState.contentState.currentItemId;
    contentState.initialize(openItems, currentItemId);
    this.verbose() && console.log(`📋 MainPanel: Synced ${openItems.length} items from server to global contentState`);
  }

  /**
   * Setup header controls event listeners
   */
  private setupHeaderControlsListeners(): void {
    // Listen to theme-clicked event - opens Theme tab with integrated AI chat
    this.addEventListener('theme-clicked', () => {
      this.verbose() && console.log('🎨 MainPanel: Theme button clicked - opening Theme tab');
      // Opens theme-widget with embedded chat for AI assistance
      // The Theme room (DEFAULT_ROOMS.THEME) provides the chat backend
      this.openContentTab('theme', 'Theme');
    });

    // Listen to settings-clicked event - opens Settings tab with integrated AI chat
    this.addEventListener('settings-clicked', () => {
      this.verbose() && console.log('⚙️ MainPanel: Settings button clicked - opening Settings tab');
      // Opens settings-widget with embedded chat for AI assistance
      // The Settings room (DEFAULT_ROOMS.SETTINGS) provides the chat backend
      this.openContentTab('settings', 'Settings');
    });

    // Listen to help-clicked event - opens Help tab with integrated AI chat
    this.addEventListener('help-clicked', () => {
      this.verbose() && console.log('❓ MainPanel: Help button clicked - opening Help tab');
      // Opens help-widget with embedded chat for AI assistance
      // The Help room (DEFAULT_ROOMS.HELP) provides the chat backend
      this.openContentTab('help', 'Help');
    });

    // Listen to browser-clicked event - opens Browser tab
    this.addEventListener('browser-clicked', () => {
      this.verbose() && console.log('🌐 MainPanel: Browser button clicked - opening Browser tab');
      this.openContentTab('browser', 'Browser');
    });

    this.verbose() && console.log('🔗 MainPanel: Header controls listeners registered');
  }

  /**
   * Subscribe to content events (opened, closed, switched) and room selection
   */
  private subscribeToContentEvents(): void {
    // Use Positron adapter for content:opened/closed/switched events
    // This delegates to PositronContentStateAdapter which updates local state
    // directly from event data instead of refetching from DB
    this.contentStateAdapter.subscribeToEvents();

    // Subscribe to pageState changes for IMMEDIATE view switching
    // This handles optimistic updates from ContentTabsWidget
    pageState.subscribe((state) => {
      if (state?.contentType) {
        // Guard: only switch if different from current view
        if (state.contentType !== this.currentViewType ||
            state.entityId !== this.currentViewEntityId) {
          this.switchContentView(state.contentType, state.entityId);
        }
      }
    });

    this.verbose() && console.log('🔗 MainPanel: Subscribed to content events and pageState');
  }

  /**
   * Refresh tabs by fetching from database
   * Used as fallback when Positron local state isn't available
   */
  private async refreshTabsFromDatabase(source: string): Promise<void> {
    try {
      this.verbose() && console.log(`📋 MainPanel: Refreshing tabs from DB (${source})...`);
      await this.loadUserContext();
      // Sync userState to global contentState - ContentTabsWidget will re-render
      this.syncUserStateToContentState();
      this.verbose() && console.log(`✅ MainPanel: Tabs refreshed from DB (${source}), now ${contentState.openItems.length} items`);
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
      this.verbose() && console.log(`📄 MainPanel: Loaded content info for ${this.currentPath}:`, this.currentContent?.displayName);
    } catch (error) {
      console.error(`❌ MainPanel: Failed to load content for ${this.currentPath}:`, error);
      this.currentContent = null;
    }
  }

  /**
   * Switch to a different content page
   */
  switchToPage(pageName: string): void {
    this.verbose() && console.log(`📄 MainPanel: Switching to page: ${pageName}`);
    // Will update the content view to show different widgets
  }

  /**
   * Open a content tab (e.g., settings, help) or switch to it if already open
   *
   * Uses global contentState - updates state, ContentTabsWidget re-renders automatically.
   */
  private openContentTab(contentType: string, title: string): void {
    const userId = this.userState?.userId;
    if (!userId) {
      console.error('❌ MainPanel: Cannot open tab - userState not loaded');
      return;
    }

    // Check if tab already exists in global state
    const existingTab = contentState.findItem(contentType, undefined);

    if (existingTab) {
      // Tab exists - set as current in global state
      contentState.setCurrent(existingTab.id);

      // Set pageState for MainWidget view switching
      pageState.setContent(contentType, existingTab.entityId, undefined);

      // Switch view
      this.switchContentView(contentType, existingTab.entityId);
      this.updateUrl(buildContentPath(contentType, existingTab.entityId));

      // Persist in background
      Commands.execute<StateContentSwitchParams, StateContentSwitchResult>('state/content/switch', {
        userId: userId as UUID,
        contentItemId: existingTab.id as UUID
      }).catch(err => console.error('Failed to persist tab switch:', err));

      this.verbose() && console.log(`📋 MainPanel: Switched to existing ${contentType} tab`);
      return;
    }

    // NEW TAB: Add to global contentState (generates temp ID)
    const newItem = contentState.addItem({
      type: contentType as ContentType,
      entityId: undefined,
      title: title,
      priority: 'normal' as ContentPriority
    }, true);

    // Set pageState for widgets
    pageState.setContent(contentType, undefined, undefined);

    // Switch view
    this.switchContentView(contentType, undefined);
    this.updateUrl(buildContentPath(contentType, undefined));

    // Persist in background - command will create real ID
    Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
      userId: userId as UUID,
      contentType: contentType as ContentType,
      title: title,
      setAsCurrent: true
    }).then(result => {
      // Update temp ID with real ID from server
      if (result?.contentItemId) {
        contentState.updateItemId(newItem.id, result.contentItemId);
      }
    }).catch(err => console.error(`Failed to open ${contentType} tab:`, err));

    this.verbose() && console.log(`📋 MainPanel: Opened new ${contentType} tab with optimistic update`);
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry