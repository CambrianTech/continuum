/**
 * MainWidget - Main Content Area Widget
 *
 * Contains content header with tabs, version info, status buttons,
 * and the main content view that displays different page widgets.
 *
 * Uses ReactiveWidget with Lit templates for efficient rendering.
 * Widget caching preserves child widget state during tab switches.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { ContentInfoManager, ContentInfo } from './shared/ContentTypes';
import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { UI_EVENTS } from '../../system/core/shared/EventConstants';
import { COMMANDS } from '../../shared/generated-command-constants';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { ROOM_UNIQUE_IDS } from '../../system/data/constants/RoomConstants';
import { getWidgetForType, buildContentPath, parseContentPath, getRightPanelConfig, initializeRecipeLayouts } from './shared/ContentTypeRegistry';
import { PositronContentStateAdapter } from '../shared/services/state/PositronContentStateAdapter';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { RoutingService } from '../../system/routing/RoutingService';
import { pageState } from '../../system/state/PageStateService';
import { contentState } from '../../system/state/ContentStateService';
import { ContentService } from '../../system/state/ContentService';
import { styles as MAIN_STYLES } from './public/main-panel.styles';

export class MainWidget extends ReactiveWidget {
  // Static styles using compiled SCSS
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(MAIN_STYLES)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private currentPath = `/chat/${ROOM_UNIQUE_IDS.GENERAL}`;

  // Non-reactive state (internal tracking)
  private contentManager!: ContentInfoManager;
  private currentContent: ContentInfo | null = null;
  private contentStateAdapter!: PositronContentStateAdapter;
  private currentViewType: string | null = null;
  private currentViewEntityId: string | undefined = undefined;

  // Widget cache - persist widgets instead of destroying them on tab switch
  private widgetCache = new Map<string, HTMLElement>();

  constructor() {
    super({
      widgetName: 'MainWidget'
    });
  }

  // === LIFECYCLE ===

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();
    this.log('Initializing main content panel...');

    // User context loaded automatically by ReactiveWidget.connectedCallback()
    // Initialize content manager with widget context
    this.contentManager = new ContentInfoManager(this);

    // Initialize Positron content state adapter
    const offMainThread = (fn: () => void, timeout = 500) => {
      if ('requestIdleCallback' in window) {
        (window as any).requestIdleCallback(fn, { timeout });
      } else {
        setTimeout(fn, 0);
      }
    };

    this.contentStateAdapter = new PositronContentStateAdapter(
      () => this.userState,
      {
        name: 'MainWidget',
        onStateChange: () => offMainThread(() => this.syncUserStateToContentState(), 1000),
        onViewSwitch: (contentType, entityId) => offMainThread(() => this.switchContentView(contentType, entityId)),
        onUrlUpdate: (contentType, identifier) => {
          queueMicrotask(() => {
            const newPath = buildContentPath(contentType, identifier);
            this.updateUrl(newPath);
          });
        },
        onFallback: () => offMainThread(() => this.refreshTabsFromDatabase('fallback'), 2000)
      }
    );

    // Load recipe layouts early
    await initializeRecipeLayouts();

    // Initialize content tabs
    await this.initializeContentTabs();

    // Listen to header controls events
    this.setupHeaderControlsListeners();

    // Subscribe to content events
    this.subscribeToContentEvents();

    // Setup URL routing
    this.setupUrlRouting();

    // Track tab visibility for temperature
    this.setupVisibilityTracking();

    this.log('Main panel initialized');
  }

  // === RENDER ===

  protected override renderContent(): TemplateResult {
    return html`
      <div class="main-container">
        <!-- Header Controls Row -->
        <div class="header-controls-row">
          <header-controls-widget></header-controls-widget>
        </div>

        <!-- Tabs Row -->
        <div class="content-tabs-row">
          <content-tabs-widget></content-tabs-widget>
        </div>

        <!-- Main Content View - widgets injected by switchContentView() -->
        <div class="content-view"></div>

        <!-- Footer -->
        <div class="content-footer">
          <div class="footer-links">
            <a href="#tos">Terms of Service</a>
            <a href="#privacy">Privacy Policy</a>
            <a href="#about">About</a>
          </div>
        </div>
      </div>
    `;
  }

  // === URL ROUTING ===

  private setupUrlRouting(): void {
    // Handle browser back/forward
    this.createMountEffect(() => {
      const handler = (event: PopStateEvent) => {
        const path = event.state?.path || window.location.pathname;
        this.navigateToPath(path);
      };
      window.addEventListener('popstate', handler);
      return () => window.removeEventListener('popstate', handler);
    });

    // Initialize from current URL
    let initialPath = window.location.pathname;

    // Default route: / or /chat without room → /chat/general
    const defaultPath = `/chat/${ROOM_UNIQUE_IDS.GENERAL}`;
    if (!initialPath || initialPath === '/' || initialPath === '/chat' || initialPath === '/chat/') {
      initialPath = defaultPath;
      window.history.replaceState({ path: initialPath }, '', initialPath);
      this.log(`Redirected to default route: ${initialPath}`);
    }

    this.currentPath = initialPath;
    const { type, entityId } = parseContentPath(initialPath);
    this.log(`Initial route: ${type}/${entityId || 'default'}`);

    // Delay slightly to let the DOM render first
    setTimeout(async () => {
      // Use ContentService for centralized content/tab/URL management
      await this.openContentFromUrl(type, entityId);
    }, 100);
  }

  /**
   * Open content from URL - uses ContentService (centralized)
   * Resolves identifier (could be uniqueId like "general" or UUID) to canonical form
   */
  private async openContentFromUrl(contentType: string, identifier?: string): Promise<void> {
    // 0. Ensure ContentService has userId for persistence
    // Wait briefly for userState if not yet loaded (race condition with loadUserContext)
    let userId = this.userState?.userId;
    if (!userId) {
      // Wait up to 500ms for userState to load
      for (let i = 0; i < 5 && !userId; i++) {
        await new Promise(resolve => setTimeout(resolve, 100));
        userId = this.userState?.userId;
      }
    }
    if (userId) {
      ContentService.setUserId(userId as UUID);
    } else {
      console.warn('⚠️ MainWidget: userState not loaded, content will not persist to database');
    }

    // 1. Resolve identifier to canonical UUID, uniqueId, displayName
    const resolved = identifier
      ? await RoutingService.resolve(contentType, identifier)
      : undefined;

    const canonicalEntityId = resolved?.id || identifier;

    // 2. Check for existing tab with this entityId
    const existingTab = this.userState?.contentState?.openItems?.find(
      item => item.type === contentType && item.entityId === canonicalEntityId
    );

    if (existingTab) {
      // Tab exists - just switch to it via ContentService
      ContentService.switchTo(existingTab.id);
      this.switchContentView(contentType, canonicalEntityId);
      return;
    }

    // 3. No existing tab - create via ContentService (centralized)
    ContentService.open(contentType, canonicalEntityId, {
      uniqueId: resolved?.uniqueId || identifier,
      title: resolved?.displayName,
      setAsCurrent: true
    });

    this.switchContentView(contentType, canonicalEntityId);
    this.log(`Opened ${contentType}/${resolved?.uniqueId || identifier || 'default'}`);
  }

  // === CONTENT VIEW SWITCHING ===

  /**
   * Switch content view to render the appropriate widget
   *
   * Widget caching with hide/show instead of destroy/recreate:
   * - Widgets are created once and cached
   * - Tab switching = hide old widget, show new widget (instant)
   * - State changes via direct method call, NOT attribute setting
   */
  private switchContentView(contentType: string, entityId?: string): void {
    // GUARD: Prevent infinite re-render loops
    if (this.currentViewType === contentType && this.currentViewEntityId === entityId) {
      return;
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
        if ('onDeactivate' in widget && typeof (widget as any).onDeactivate === 'function') {
          (widget as any).onDeactivate();
        }
        this.log(`Deactivated ${tag}`);
      }
    });

    // === GET OR CREATE widget ===
    let widget = this.widgetCache.get(widgetTag);

    if (!widget) {
      const existingInDom = contentView.querySelector(widgetTag) as HTMLElement;
      if (existingInDom) {
        widget = existingInDom;
        this.widgetCache.set(widgetTag, widget);
        this.log(`Cached existing ${widgetTag} from template`);
      } else {
        widget = document.createElement(widgetTag);
        widget.style.display = 'none';
        contentView.appendChild(widget);
        this.widgetCache.set(widgetTag, widget);
        this.log(`Created and cached ${widgetTag}`);
      }
    }

    // === ACTIVATE widget (show + notify) ===
    widget.style.display = '';

    // Look up ContentItem to get metadata (may contain pre-loaded entity)
    const contentItem = contentState.findItem(contentType, entityId);
    const metadata = contentItem?.metadata;

    if ('onActivate' in widget && typeof (widget as any).onActivate === 'function') {
      (widget as any).onActivate(entityId, metadata);
    } else if ('setEntityId' in widget && typeof (widget as any).setEntityId === 'function') {
      (widget as any).setEntityId(entityId);
    } else {
      if (entityId) {
        widget.setAttribute('entity-id', entityId);
      } else {
        widget.removeAttribute('entity-id');
      }
    }

    // Emit right panel configuration
    const rightPanelConfig = getRightPanelConfig(contentType);
    Events.emit(UI_EVENTS.RIGHT_PANEL_CONFIGURE, {
      widget: rightPanelConfig?.widget || null,
      room: rightPanelConfig?.room,
      compact: rightPanelConfig?.compact,
      contentType: contentType
    });

    // Emit Positron widget state
    PositronWidgetState.emit({
      widgetType: contentType,
      entityId: entityId,
      title: entityId ? `${contentType} - ${entityId}` : contentType,
      metadata: {
        widget: widgetTag,
        rightPanelRoom: rightPanelConfig?.room
      }
    });

    this.log(`Rendered ${widgetTag} for ${contentType}${entityId ? ` (${entityId})` : ''}`);
  }

  private updateUrl(path: string): void {
    if (this.currentPath !== path) {
      this.currentPath = path;
      window.history.pushState({ path }, '', path);
    }
  }

  // === NAVIGATION ===

  async navigateToPath(newPath: string): Promise<void> {
    const { type, entityId } = parseContentPath(newPath);

    if (type === 'chat' && entityId) {
      await this.ensureRoomExists(entityId);
    }

    this.currentPath = newPath;

    // Use centralized method - one logical decision, one place
    await this.openContentFromUrl(type, entityId);
  }

  private async ensureRoomExists(roomId: string): Promise<void> {
    try {
      const content = await this.contentManager.getContentByPath(`/chat/${roomId}`);

      if (!content) {
        const roomType = roomId.startsWith('user-') ? 'user_chat' : 'private';
        await this.contentManager.createRoom(roomId, roomType);
        this.log(`Created new room: ${roomId} (${roomType})`);
      } else {
        this.log(`Room exists: ${roomId} (${content.displayName})`);
      }
    } catch (error) {
      console.error(`❌ MainPanel: Failed to ensure room ${roomId} exists:`, error);
    }
  }

  // === VISIBILITY TRACKING ===

  private setupVisibilityTracking(): void {
    this.createMountEffect(() => {
      const handler = async () => {
        const [, pathType, roomId] = this.currentPath.split('/');

        if (pathType === 'chat' && roomId) {
          const present = !document.hidden;

          try {
            await Commands.execute(COMMANDS.COLLABORATION_ACTIVITY_USER_PRESENT, {
              activityId: roomId,
              present
            } as any);
            this.log(`User ${present ? 'present' : 'left'} in room ${roomId}`);
          } catch (error) {
            const isDisconnected = error instanceof Error &&
              (error.message.includes('WebSocket not ready') || error.message.includes('WebSocket not connected'));
            if (!isDisconnected) {
              console.error('❌ MainPanel: Failed to track visibility:', error);
            }
          }
        }
      };

      document.addEventListener('visibilitychange', handler);
      return () => document.removeEventListener('visibilitychange', handler);
    });

    this.log('Visibility tracking initialized');
  }

  // === CONTENT STATE ===

  private async initializeContentTabs(): Promise<void> {
    // Wait for userState to load (race condition with loadUserContext)
    let userStateLoaded = this.userState?.contentState;
    console.log(`🔍 initializeContentTabs: Initial check - hasUserState=${!!this.userState}, hasContentState=${!!userStateLoaded}`);

    if (!userStateLoaded) {
      // Wait up to 2 seconds for userState to load (increased from 1s)
      for (let i = 0; i < 20 && !userStateLoaded; i++) {
        await new Promise(resolve => setTimeout(resolve, 100));
        userStateLoaded = this.userState?.contentState;
        if (i === 9) {
          console.log(`🔍 initializeContentTabs: Still waiting (${i*100}ms) - hasUserState=${!!this.userState}, hasContentState=${!!userStateLoaded}`);
        }
      }
    }

    if (userStateLoaded) {
      const openItems = this.userState!.contentState.openItems || [];
      const currentItemId = this.userState!.contentState.currentItemId;
      console.log(`✅ initializeContentTabs: Found ${openItems.length} items, currentItemId=${currentItemId}`);
      contentState.initialize(openItems, currentItemId);
      this.log(`Initialized global contentState with ${openItems.length} items`);
    } else {
      console.log(`⚠️ initializeContentTabs: UserState not loaded after 2s - userId might be wrong or DB query failed`);
      this.log('⚠️ UserState not loaded after 2s, starting with empty tabs');
      contentState.initialize([], undefined);
    }
  }

  private syncUserStateToContentState(): void {
    if (!this.userState?.contentState) return;

    const openItems = this.userState.contentState.openItems || [];
    const currentItemId = this.userState.contentState.currentItemId;
    contentState.update(openItems, currentItemId);
    this.log(`Synced ${openItems.length} items from server to global contentState`);
  }

  // === HEADER CONTROLS ===

  private setupHeaderControlsListeners(): void {
    this.createMountEffect(() => {
      const themeHandler = () => {
        this.log('Theme button clicked - opening Theme tab');
        this.openContentTab('theme', 'Theme');
      };

      const settingsHandler = () => {
        this.log('Settings button clicked - opening Settings tab');
        this.openContentTab('settings', 'Settings');
      };

      const helpHandler = () => {
        this.log('Help button clicked - opening Help tab');
        this.openContentTab('help', 'Help');
      };

      const browserHandler = () => {
        this.log('Browser button clicked - opening Browser tab');
        this.openContentTab('browser', 'Browser');
      };

      this.addEventListener('theme-clicked', themeHandler);
      this.addEventListener('settings-clicked', settingsHandler);
      this.addEventListener('help-clicked', helpHandler);
      this.addEventListener('browser-clicked', browserHandler);

      return () => {
        this.removeEventListener('theme-clicked', themeHandler);
        this.removeEventListener('settings-clicked', settingsHandler);
        this.removeEventListener('help-clicked', helpHandler);
        this.removeEventListener('browser-clicked', browserHandler);
      };
    });

    this.log('Header controls listeners registered');
  }

  // === CONTENT EVENTS ===

  private subscribeToContentEvents(): void {
    this.contentStateAdapter.subscribeToEvents();

    this.createMountEffect(() => {
      const unsubscribe = pageState.subscribe((state) => {
        if (state?.contentType) {
          if (state.contentType !== this.currentViewType ||
              state.entityId !== this.currentViewEntityId) {
            this.switchContentView(state.contentType, state.entityId);
          }
        }
      });
      return () => unsubscribe();
    });

    // Handle navigate:live events from chat/user widgets
    this.createMountEffect(() => {
      const unsubscribe = Events.subscribe('navigate:live', (data: { entityId: string; entityType: string; displayName?: string }) => {
        this.log(`Navigate to live: ${data.entityType}/${data.entityId}`);
        const userId = this.userState?.userId;
        if (userId) {
          ContentService.setUserId(userId as UUID);
        }
        ContentService.open('live', data.entityId, {
          title: data.displayName || 'Live Call',
          setAsCurrent: true
        });
      });
      return () => unsubscribe();
    });

    this.log('Subscribed to content events and pageState');
  }

  private async refreshTabsFromDatabase(source: string): Promise<void> {
    try {
      this.log(`Refreshing tabs from DB (${source})...`);
      await this.loadUserContext();
      this.syncUserStateToContentState();
      this.log(`Tabs refreshed from DB (${source}), now ${contentState.openItems.length} items`);
    } catch (error) {
      console.error(`❌ MainPanel: Error refreshing tabs from DB (${source}):`, error);
    }
  }

  private async loadCurrentContent(): Promise<void> {
    try {
      this.currentContent = await this.contentManager.getContentByPath(this.currentPath);
      this.log(`Loaded content info for ${this.currentPath}: ${this.currentContent?.displayName}`);
    } catch (error) {
      console.error(`❌ MainPanel: Failed to load content for ${this.currentPath}:`, error);
      this.currentContent = null;
    }
  }

  switchToPage(pageName: string): void {
    this.log(`Switching to page: ${pageName}`);
  }

  /**
   * Open a content tab (settings, theme, help, browser)
   * Delegates to ContentService - single source of truth for content operations
   */
  private openContentTab(contentType: string, title: string): void {
    const userId = this.userState?.userId;
    if (!userId) {
      console.error('❌ MainPanel: Cannot open tab - userState not loaded');
      return;
    }

    // Ensure ContentService has the userId
    ContentService.setUserId(userId as UUID);

    // Check for existing tab of this type
    const existingTab = contentState.findItem(contentType, undefined);

    if (existingTab) {
      // Switch to existing tab via ContentService (single source of truth)
      ContentService.switchTo(existingTab.id);
      this.log(`Switched to existing ${contentType} tab`);
      return;
    }

    // Open new tab via ContentService (single source of truth)
    ContentService.open(contentType, undefined, { title, setAsCurrent: true });
    this.log(`Opened new ${contentType} tab`);
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
