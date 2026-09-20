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
import { Events } from '../../system/core/shared/Events';
import { jtagGlobal } from '../../system/core/types/GlobalAugmentations';
import { UI_EVENTS } from '../../system/core/shared/EventConstants';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import type { ContentItem } from '../../system/data/entities/UserStateEntity';
import { COLLECTIONS } from '../../system/shared/Constants';
import { DATA_COMMANDS } from '../../commands/data/shared/DataCommandConstants';
import type { DataUpdateParams, DataUpdateResult } from '../../commands/data/update/shared/DataUpdateTypes';
import '../onboarding/WelcomeModalWidget';
import { getWidgetForType, buildContentPath, parseContentPath, getRightPanelConfig, initializeRecipeLayouts } from './shared/ContentTypeRegistry';
import { PositronContentStateAdapter } from '../shared/services/state/PositronContentStateAdapter';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import { RoutingService } from '../../system/routing/RoutingService';
import { pageState } from '../../system/state/PageStateService';
import { contentState } from '../../system/state/ContentStateService';
import { ContentService } from '../../system/state/ContentService';
import { isContentViewWidget } from '../../system/state/ContentLifecycle';
import { ActivityUserPresent } from '../../commands/collaboration/activity/user-present/shared/ActivityUserPresentTypes';
import { styles as MAIN_STYLES } from './public/main-panel.styles';

export class MainWidget extends ReactiveWidget {
  // Static styles using compiled SCSS
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(MAIN_STYLES)
  ] as CSSResultGroup;

  // Reactive state
  // Joel 2026-05-03: was defaulted to `/chat/general` — same phantom-tab
  // antipattern. setupUrlRouting() sets currentPath from the actual URL.
  @reactive() private currentPath = '';

  // First-run welcome (#1101). True when the current user's
  // `UserEntity.hasOnboarded` is falsy. Set in onFirstRender after
  // user context loads; cleared when the modal completes.
  @reactive() private _showWelcome = false;

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
        window.requestIdleCallback(fn, { timeout });
      } else {
        setTimeout(fn, 0);
      }
    };

    this.contentStateAdapter = new PositronContentStateAdapter(
      () => this.userState,
      {
        name: 'MainWidget',
        onStateChange: () => offMainThread(() => {
          void this.syncUserStateToContentState()
            .catch(error => console.error('❌ MainWidget: syncUserStateToContentState failed:', error));
        }, 1000),
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

    // Re-emit right panel config now that BOTH recipes and content tabs are loaded.
    // Fixes race: content tab may have rendered before recipes were available,
    // so the right panel got null instead of the recipe's config.
    // Re-emit right panel for ALL active content tabs, not just the current one.
    // The initial render may have happened before recipes loaded.
    if (this.currentViewType) {
      const rightPanelConfig = getRightPanelConfig(this.currentViewType);
      Events.emit(UI_EVENTS.RIGHT_PANEL_CONFIGURE, {
        widget: rightPanelConfig?.widget || null,
        room: rightPanelConfig?.room,
        compact: rightPanelConfig?.compact,
        contentType: this.currentViewType,
        sections: rightPanelConfig?.sections,
      });
    }

    // Listen to header controls events
    this.setupHeaderControlsListeners();

    // Subscribe to content events
    this.subscribeToContentEvents();

    // Setup URL routing
    this.setupUrlRouting();

    // Track tab visibility for temperature
    this.setupVisibilityTracking();

    // First-run welcome (#1101). currentUser is populated by
    // ReactiveWidget.connectedCallback() before onFirstRender runs.
    // Falsy `hasOnboarded` (including undefined on existing rows
    // pre-migration) opens the modal.
    if (this.currentUser && !this.currentUser.hasOnboarded) {
      this._showWelcome = true;
    }

    this.log('Main panel initialized');
  }

  /**
   * Fired when the user advances past the final welcome panel — or
   * dismisses the modal. Either way, mark the user onboarded so the
   * modal doesn't re-appear on the next session. Failure to persist
   * just means the modal shows again next time; not worth surfacing.
   */
  private async onWelcomeComplete(): Promise<void> {
    this._showWelcome = false;
    const user = this.currentUser;
    if (!user?.id) return;
    try {
      await this.executeCommand<DataUpdateParams, DataUpdateResult>(DATA_COMMANDS.UPDATE, {
        collection: COLLECTIONS.USERS,
        id: user.id,
        data: { hasOnboarded: true },
        backend: 'server',
        dbHandle: 'default',
      });
      // Reflect immediately on the in-memory entity so a hot re-render
      // (e.g. theme switch) doesn't re-open the modal before the next
      // page load reloads currentUser from the server.
      user.hasOnboarded = true;
    } catch (err) {
      console.warn('MainWidget: failed to persist hasOnboarded — modal will re-show next session', err);
    }
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

        <!-- First-run welcome (#1101). Self-positions via fixed/z-index
             so its placement in the DOM doesn't matter; lives at the
             container's bottom for theme variable inheritance. -->
        <welcome-modal
          ?open=${this._showWelcome}
          @welcome-complete=${() => this.onWelcomeComplete()}
        ></welcome-modal>
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
    const initialPath = window.location.pathname;
    this.currentPath = initialPath;

    // Joel 2026-05-03: NO default tab on root. The previous redirect from
    // `/` → `/chat/general` was the source of the phantom "General" tab
    // that appeared with a stale UUID + "Loading members..." forever
    // (same antipattern family as the long-fixed stringToUUID('General')
    // ghost — see system/data/domains/DefaultEntities.ts header). Empty
    // root means empty content area; persisted tabs (if any) restore
    // via initializeContentTabs() above and the user picks from the
    // sidebar / opens what they want.
    const isRootPath = !initialPath || initialPath === '/' || initialPath === '/chat' || initialPath === '/chat/';
    if (isRootPath) {
      this.log('Root path — no default tab; persisted tabs (if any) restore from contentState');
      return;
    }

    const { type, entityId } = parseContentPath(initialPath);
    if (!type) {
      this.log(`Unrecognized initial route '${initialPath}' — no tab opened`);
      return;
    }
    this.log(`Initial route: ${type}/${entityId || 'default'}`);

    // Wait for JTAG client to be connected before resolving routes.
    // On page reload, the WebSocket needs time to reconnect. Without this,
    // RoutingService.resolve() fails silently because Commands can't execute.
    // Wait for JTAG client — must be long enough for Docker startup race.
    // WS connect() retries for up to 60s, so this must wait at least that long.
    const waitForClient = async () => {
      for (let i = 0; i < 300; i++) {
        if (jtagGlobal.jtag) return;
        await new Promise(r => setTimeout(r, 200));
      }
    };
    setTimeout(async () => {
      await waitForClient();

      // Always open from URL. The URL is the source of truth for the initial view.
      // Persisted tabs are already loaded into contentState via initializeContentTabs().
      // openContentFromUrl will find existing matching tabs and switch to them,
      // or create a new tab if none match.
      try {
        await this.openContentFromUrl(type, entityId);
      } catch (err) {
        console.error(`❌ MainWidget: openContentFromUrl failed for ${type}/${entityId}:`, err);
      }
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
      ContentService.setUserId(userId);
    } else {
      console.warn('⚠️ MainWidget: userState not loaded, content will not persist to database');
    }

    // 1. Resolve identifier to canonical UUID, uniqueId, displayName
    // Resolve can fail after page reload if the command system isn't ready yet.
    // Fall through with identifier as-is — the widget can resolve later.
    let resolved: Awaited<ReturnType<typeof RoutingService.resolve>> | undefined;
    if (identifier) {
      // Retry resolve until it succeeds — don't create a broken tab with a raw UUID.
      // The WS connection may not be ready yet on first load (Docker startup race).
      for (let attempt = 0; attempt < 30; attempt++) {
        try {
          resolved = (await RoutingService.resolve(contentType, identifier)) ?? undefined;
          if (resolved) break;
        } catch {
          // WS not ready yet — wait and retry
        }
        await new Promise(r => setTimeout(r, 2000));
      }
      if (!resolved) {
        console.warn(`⚠️ MainWidget: RoutingService.resolve failed after 30 retries for ${contentType}/${identifier} — not opening broken tab`);
        return; // Don't create a broken tab
      }
    }

    const canonicalEntityId = resolved?.id || identifier;

    // 2. Check for existing tab — use the GLOBAL contentState singleton (runtime truth),
    // not this.userState.contentState (server-persisted, may be stale or not yet synced).
    // Reading from two different contentState objects was the source of duplicate tabs:
    // initializeContentTabs writes to the global, but this check read from userState.
    const existingTab = contentState.findItem(contentType, canonicalEntityId)
      || (identifier !== canonicalEntityId ? contentState.findItem(contentType, identifier) : null);

    if (existingTab) {
      // Tab exists - switch to it, using the resolved UUID for the widget
      const entityForWidget = resolved?.id || existingTab.entityId || canonicalEntityId;
      ContentService.switchTo(existingTab.id);
      this.switchContentView(contentType, entityForWidget);
      return;
    }

    // 3. No existing tab - create via ContentService (centralized)
    // Format title based on content type (genome-profile → "DeepSeek Genome")
    let title = resolved?.displayName;
    if (title && contentType === 'genome-profile') {
      title = `${title} Genome`;
    }

    ContentService.open(contentType, canonicalEntityId, {
      uniqueId: resolved?.uniqueId || identifier,
      title,
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
        if (isContentViewWidget(widget) && widget.onDeactivate) {
          widget.onDeactivate();
        }
        this.log(`Deactivated ${tag}`);
      }
    });

    // === GET OR CREATE widget ===
    // Cache key includes entityId for entity-backed widgets (e.g., chat-widget per room)
    // so each room gets its own widget instance with its own state.
    const cacheKey = entityId ? `${widgetTag}:${entityId}` : widgetTag;
    let widget = this.widgetCache.get(cacheKey);

    if (!widget) {
      const existingInDom = !entityId ? contentView.querySelector(widgetTag) as HTMLElement : null;
      if (existingInDom) {
        widget = existingInDom;
        this.widgetCache.set(cacheKey, widget);
        this.log(`Cached existing ${widgetTag} from template`);
      } else {
        widget = document.createElement(widgetTag);
        widget.style.display = 'none';
        contentView.appendChild(widget);
        this.widgetCache.set(cacheKey, widget);
        this.log(`Created ${widgetTag} for ${entityId || 'singleton'}`);
      }
    }

    // === ACTIVATE widget (show + notify) ===
    widget.style.display = '';

    // Look up ContentItem to get metadata (may contain pre-loaded entity)
    const contentItem = contentState.findItem(contentType, entityId);
    const metadata = contentItem?.metadata;

    if (isContentViewWidget(widget)) {
      if (widget.onActivate) {
        widget.onActivate(entityId, metadata);
      } else if (widget.setEntityId && entityId) {
        widget.setEntityId(entityId);
      }
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
      contentType: contentType,
      sections: rightPanelConfig?.sections,
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

  private clearContentView(): void {
    this.widgetCache.forEach((widget, tag) => {
      if (widget.style.display !== 'none') {
        widget.style.display = 'none';
        if (isContentViewWidget(widget) && widget.onDeactivate) {
          widget.onDeactivate();
        }
        this.log(`Deactivated ${tag}`);
      }
    });
    this.currentViewType = null;
    this.currentViewEntityId = undefined;
    Events.emit(UI_EVENTS.RIGHT_PANEL_CONFIGURE, {
      widget: null,
      contentType: null
    });
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
    if (!type) {
      this.log(`Unrecognized navigation path '${newPath}' — ignoring`);
      return;
    }

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
            await ActivityUserPresent.execute({
              activityId: roomId as UUID,
              present
            });
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
      const rawOpenItems = this.userState!.contentState.openItems || [];
      const rawCurrentItemId = this.userState!.contentState.currentItemId;
      const { openItems, currentItemId } = await this.sanitizePersistedContentItems(rawOpenItems, rawCurrentItemId);
      console.log(`✅ initializeContentTabs: Found ${rawOpenItems.length} items, using ${openItems.length}, currentItemId=${currentItemId}`);
      contentState.initialize(openItems, currentItemId);
      this.log(`Initialized global contentState with ${openItems.length} items`);
    } else {
      console.log(`⚠️ initializeContentTabs: UserState not loaded after 2s - userId might be wrong or DB query failed`);
      this.log('⚠️ UserState not loaded after 2s, starting with empty tabs');
      contentState.initialize([], undefined);
    }
  }

  private async syncUserStateToContentState(): Promise<void> {
    if (!this.userState?.contentState) return;

    const { openItems, currentItemId } = await this.sanitizePersistedContentItems(
      this.userState.contentState.openItems || [],
      this.userState.contentState.currentItemId
    );
    contentState.update(openItems, currentItemId);
    this.log(`Synced ${openItems.length} items from server to global contentState`);
  }

  private async sanitizePersistedContentItems(openItems: ContentItem[], currentItemId?: UUID): Promise<{
    openItems: ContentItem[];
    currentItemId?: UUID;
  }> {
    type ValidationResult =
      | { status: 'keep'; item: ContentItem }
      | { status: 'drop'; item: ContentItem };

    const validatedItems = await Promise.all(openItems.map(async (item): Promise<ValidationResult> => {
      const identifier = item.uniqueId || item.entityId;
      if (!identifier || !ContentService.getCollectionForContentType(item.type)) {
        return { status: 'keep', item };
      }

      let resolved: Awaited<ReturnType<typeof RoutingService.resolve>> | null = null;
      try {
        resolved = await RoutingService.resolve(item.type, identifier);
        if (!resolved && item.entityId && item.entityId !== identifier) {
          resolved = await RoutingService.resolve(item.type, item.entityId);
        }
      } catch (error) {
        console.warn(`⚠️ MainWidget: could not validate persisted ${item.type}/${identifier}:`, error);
        return { status: 'keep', item };
      }

      if (!resolved) {
        console.warn(`⚠️ MainWidget: dropping stale persisted tab ${item.type}/${identifier} (${item.title})`);
        return { status: 'drop', item };
      }

      return {
        status: 'keep',
        item: {
          ...item,
          entityId: resolved.id,
          uniqueId: resolved.uniqueId,
          title: resolved.displayName || item.title,
        }
      };
    }));

    const sanitized = validatedItems
      .filter((result): result is Extract<ValidationResult, { status: 'keep' }> => result.status === 'keep')
      .map(result => result.item);

    const deduped: ContentItem[] = [];
    const duplicateCurrentTargets = new Map<UUID, UUID>();
    for (const item of sanitized) {
      const existing = deduped.find(candidate => {
        const candidatePath = buildContentPath(candidate.type, candidate.uniqueId || candidate.entityId);
        const itemPath = buildContentPath(item.type, item.uniqueId || item.entityId);
        return candidatePath === itemPath;
      });
      if (existing) {
        duplicateCurrentTargets.set(item.id, existing.id);
        continue;
      }
      deduped.push(item);
    }

    let resolvedCurrentItemId = currentItemId;
    if (resolvedCurrentItemId && duplicateCurrentTargets.has(resolvedCurrentItemId)) {
      resolvedCurrentItemId = duplicateCurrentTargets.get(resolvedCurrentItemId);
    }
    if (!resolvedCurrentItemId || !deduped.some(item => item.id === resolvedCurrentItemId)) {
      resolvedCurrentItemId = deduped[0]?.id;
    }

    return { openItems: deduped, currentItemId: resolvedCurrentItemId };
  }

  // === HEADER CONTROLS ===

  private setupHeaderControlsListeners(): void {
    this.createMountEffect(() => {
      const universeHandler = () => {
        this.log('Universe button clicked - opening Universe tab');
        this.openContentTab('universe', 'Universe');
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

      const trainingHandler = () => {
        this.log('Training button clicked - opening Training Dashboard');
        this.openContentTab('training-dashboard', 'Training');
      };

      const gridHandler = () => {
        this.log('Grid button clicked - opening Grid Overview');
        this.openContentTab('grid-overview', 'Grid');
      };

      const factoryHandler = () => {
        this.log('Factory button clicked - opening Factory');
        this.openContentTab('factory', 'Factory');
      };

      this.addEventListener('universe-clicked', universeHandler);
      this.addEventListener('settings-clicked', settingsHandler);
      this.addEventListener('help-clicked', helpHandler);
      this.addEventListener('browser-clicked', browserHandler);
      this.addEventListener('training-clicked', trainingHandler);
      this.addEventListener('grid-clicked', gridHandler);
      this.addEventListener('factory-clicked', factoryHandler);

      return () => {
        this.removeEventListener('universe-clicked', universeHandler);
        this.removeEventListener('settings-clicked', settingsHandler);
        this.removeEventListener('help-clicked', helpHandler);
        this.removeEventListener('browser-clicked', browserHandler);
        this.removeEventListener('training-clicked', trainingHandler);
        this.removeEventListener('grid-clicked', gridHandler);
      };
    });

    this.log('Header controls listeners registered');
  }

  // === CONTENT EVENTS ===

  private subscribeToContentEvents(): void {
    this.contentStateAdapter.subscribeToEvents();

    this.createMountEffect(() => {
      const unsubscribe = pageState.subscribe((state) => {
        if (!state) {
          this.clearContentView();
          return;
        }
        if (state.contentType) {
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
      const unsubscribe = Events.subscribe('navigate:live', (data: { entityId: string; uniqueId?: string; entityType: string; displayName?: string }) => {
        this.log(`Navigate to live: ${data.entityType}/${data.uniqueId || data.entityId}`);
        const userId = this.userState?.userId;
        if (userId) {
          ContentService.setUserId(userId);
        }
        // Use uniqueId for clean URLs (/live/general not /live/5e71a0c8-...)
        ContentService.open('live', data.entityId, {
          uniqueId: data.uniqueId || data.entityId,
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
    ContentService.setUserId(userId);

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
