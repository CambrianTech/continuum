/**
 * Chat Widget - Now extends EntityScrollerWidget for automatic CRUD management
 * Eliminates ~300 lines of manual EntityScroller and CRUD event handling
 */

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

import { EntityScrollerWidget } from '../../shared/EntityScrollerWidget';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { ChatMessageEntity, type MediaItem, type MediaType } from '../../../system/data/entities/ChatMessageEntity';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { DataReadParams, DataReadResult } from '../../../commands/data/read/shared/DataReadTypes';
import type { ChatSendParams, ChatSendResult } from '../../../commands/collaboration/chat/send/shared/ChatSendTypes';
import { Commands } from '../../../system/core/shared/Commands';
import { Events } from '../../../system/core/shared/Events';
import { SCROLLER_PRESETS, type RenderFn, type LoadFn, type ScrollerConfig } from '../../shared/EntityScroller';
import { DEFAULT_ROOMS, DEFAULT_USERS } from '../../../system/data/domains/DefaultEntities';
import { AdapterRegistry } from '../adapters/AdapterRegistry';
import { AbstractMessageAdapter } from '../adapters/AbstractMessageAdapter';
import { MessageEventDelegator } from '../adapters/MessageEventDelegator';
import { ImageMessageAdapter } from '../adapters/ImageMessageAdapter';
import { URLCardAdapter } from '../adapters/URLCardAdapter';
import { MessageInputEnhancer } from '../message-input/MessageInputEnhancer';
import { AIStatusIndicator } from './AIStatusIndicator';
import { AI_DECISION_EVENTS } from '../../../system/events/shared/AIDecisionEvents';
import { AI_LEARNING_EVENTS } from '../../../system/events/shared/AILearningEvents';
import { PositronWidgetState } from '../../shared/services/state/PositronWidgetState';
// Signals for React-like state management
import { createWidgetSignals, watch, type WidgetSignalState, type Dispose } from '@system/signals';
// EntityCacheService - single source of truth for entity data (Positronic pattern)
import { entityCache } from '../../../system/state/EntityCacheService';

/**
 * ChatWidget signal state - React-like reactive state management
 * Changes to these values automatically trigger UI updates via watch()
 */
interface ChatSignalState {
  roomId: UUID | null;
  roomUniqueId: string | null;
  roomName: string;
  isActiveContent: boolean;
  totalMessageCount: number;
  loadedMessageCount: number;
}

/**
 * Data needed to render a member chip in the header
 */
interface MemberChipData {
  personaId: UUID;
  displayName: string;
  role: string;
  roleIcon: string;
  statusEmoji: string;
  hasError: boolean;
  hasStatus: boolean;
}

export class ChatWidget extends EntityScrollerWidget<ChatMessageEntity> {
  private messageInput?: HTMLTextAreaElement;

  // === SIGNAL-BASED STATE (React-like reactivity) ===
  private _signals: WidgetSignalState<ChatSignalState>;
  private _signalDisposers: Dispose[] = []; // Cleanup for watch/effect subscriptions

  // Getters for backward compatibility - read from signals
  private get currentRoomId(): UUID | null { return this._signals.state.roomId; }
  private set currentRoomId(v: UUID | null) { this._signals.set('roomId', v); }

  private get currentRoomUniqueId(): string | null { return this._signals.state.roomUniqueId; }
  private set currentRoomUniqueId(v: string | null) { this._signals.set('roomUniqueId', v); }

  private get currentRoomName(): string { return this._signals.state.roomName; }
  private set currentRoomName(v: string) { this._signals.set('roomName', v); }

  private get totalMessageCount(): number { return this._signals.state.totalMessageCount; }
  private set totalMessageCount(v: number) { this._signals.set('totalMessageCount', v); }

  private get loadedMessageCount(): number { return this._signals.state.loadedMessageCount; }
  private set loadedMessageCount(v: number) { this._signals.set('loadedMessageCount', v); }

  // Non-signal state (doesn't need reactivity)
  private currentRoom: RoomEntity | null = null;
  private roomMembers: Map<UUID, UserEntity> = new Map();
  private adapterRegistry: AdapterRegistry;
  private eventDelegator: MessageEventDelegator;
  private aiStatusIndicator: AIStatusIndicator;
  private aiStatusContainer?: HTMLElement;
  private headerUpdateTimeout?: number;
  private errorsHidden: boolean = true;
  private pendingAttachments: MediaItem[] = [];
  private isSending: boolean = false;
  private positronUnsubscribe?: () => void;
  private positronUpdateDebounce?: number;

  // === REACT-LIKE VISIBILITY STATE ===
  private get _isActiveContent(): boolean { return this._signals.state.isActiveContent; }
  private set _isActiveContent(v: boolean) { this._signals.set('isActiveContent', v); }

  private _isPinnedWidget: boolean = false;
  private _eventUnsubscribers: Array<() => void> = [];
  private _pendingMessageTempIds: Set<UUID> = new Set();
  private _pendingRoomEntity: RoomEntity | null = null;  // For instant hydration

  constructor() {
    super({
      widgetId: 'chat-widget',
      widgetName: 'ChatWidget',
      styles: 'chat-widget.css',
      enableAI: true,
      enableDatabase: true,
      enableRouterEvents: false,
      enableScreenshots: false
    });

    // Initialize signal store with default state (React-like pattern)
    this._signals = createWidgetSignals<ChatSignalState>({
      roomId: null,
      roomUniqueId: null,
      roomName: 'General',
      isActiveContent: false,
      totalMessageCount: 0,
      loadedMessageCount: 0
    }, { widgetName: 'ChatWidget' });

    // Initialize adapter registry for per-message adapter selection
    this.adapterRegistry = new AdapterRegistry();

    // Initialize event delegator for memory-efficient message interactions
    this.eventDelegator = new MessageEventDelegator({ verbose: false });

    // Initialize AI status indicator manager
    this.aiStatusIndicator = new AIStatusIndicator();
  }

  // Static property required by widget registration system
  static get widgetName(): string {
    return 'chat';
  }

  // Observed attributes for reactive updates
  static get observedAttributes(): string[] {
    return ['compact', 'room'];
  }

  // Compact mode for right panel / mobile
  private _compact: boolean = false;

  get compact(): boolean {
    return this._compact;
  }

  set compact(value: boolean) {
    this._compact = value;
    this.updateCompactMode();
  }

  attributeChangedCallback(name: string, oldValue: string | null, newValue: string | null): void {
    if (oldValue === newValue) return;

    switch (name) {
      case 'compact':
        this._compact = newValue !== null && newValue !== 'false';
        this.updateCompactMode();
        break;
      case 'room':
        if (newValue) {
          this.switchToRoom(newValue);
        }
        break;
    }
  }

  private updateCompactMode(): void {
    const container = this.shadowRoot?.querySelector('.entity-list-container');
    if (container) {
      container.classList.toggle('compact', this._compact);
    }
  }

  /**
   * Called by MainWidget when this widget is activated with a new entityId.
   * Implements clear/populate/query pattern for instant hydration.
   */
  public async onActivate(entityId?: string, metadata?: Record<string, unknown>): Promise<void> {
    // Store room entity from metadata for instant hydration
    this._pendingRoomEntity = (metadata?.entity as RoomEntity) || null;

    if (entityId) {
      await this.switchToRoom(entityId);
    }
  }

  // === REACT-LIKE VISIBILITY TRACKING ===

  /**
   * Whether this widget is the currently active/visible chat content.
   * Use this to skip expensive operations when the widget is in background.
   */
  get isActiveContent(): boolean {
    return this._isActiveContent;
  }

  /**
   * Update active content state.
   * Called when pageState changes or content:switched fires.
   */
  private setActiveContent(isActive: boolean, reason: string): void {
    if (this._isActiveContent === isActive) return;

    this._isActiveContent = isActive;
    verbose() && console.log(`📨 ChatWidget: ${isActive ? 'ACTIVATED' : 'DEACTIVATED'} (${reason})`);

    if (isActive) {
      // When activated, ensure UI is up-to-date
      this.updateHeader();
    }
  }

  /**
   * Check if this widget should react to content/room events.
   * Pinned widgets (with room attribute) only respond to their specific room.
   * Non-pinned widgets respond to all content switching events.
   */
  private shouldReactToContentEvents(): boolean {
    // Pinned widgets ignore global content events - they're locked to one room
    return !this._isPinnedWidget;
  }

  /**
   * Subscribe to an event and track for cleanup.
   * Use this instead of Events.subscribe() directly to prevent memory leaks.
   * Captures the unsubscribe function returned by Events.subscribe().
   */
  private subscribeWithCleanup(eventName: string, handler: (data: any) => void): void {
    const unsubscribe = Events.subscribe(eventName, handler);
    if (unsubscribe && typeof unsubscribe === 'function') {
      this._eventUnsubscribers.push(unsubscribe);
    }
  }

  /**
   * Consolidated room switching logic - single source of truth for room changes.
   * Called by pageState subscription and content events (content:opened, content:switched).
   */
  private async handleRoomSwitch(roomIdOrUniqueId: string, _roomName?: string, _source?: string): Promise<void> {
    // Skip if pinned widget (handled elsewhere)
    if (this._isPinnedWidget) {
      return;
    }

    // SAME ROOM: Just refresh messages (new messages may have arrived)
    // This is critical for switching back to a tab - don't skip the refresh!
    if (roomIdOrUniqueId === this.currentRoomId || roomIdOrUniqueId === this.currentRoomUniqueId) {
      verbose() && console.log(`📨 ChatWidget: Same room, refreshing messages`);
      await this.scroller?.refresh();
      this.updateHeader();
      return;
    }

    // Use the full switchToRoom method for proper resolution
    await this.switchToRoom(roomIdOrUniqueId);
  }

  private async switchToRoom(roomIdOrName: string): Promise<void> {
    // Try to use pre-loaded entity from metadata (instant hydration)
    try {
      let roomId: UUID | undefined;
      let roomName: string = roomIdOrName;
      let roomUniqueId: string = roomIdOrName;
      let room: RoomEntity | null = null;

      // CHECK FOR PRE-LOADED ENTITY (instant hydration)
      if (this._pendingRoomEntity &&
          (this._pendingRoomEntity.id === roomIdOrName ||
           this._pendingRoomEntity.uniqueId === roomIdOrName)) {
        room = this._pendingRoomEntity;
        roomId = room.id as UUID;
        roomName = room.displayName || room.name || roomIdOrName;
        roomUniqueId = room.uniqueId || roomIdOrName;
        this._pendingRoomEntity = null; // Clear after use
      } else {
        // QUERY - only if no matching pre-loaded entity
        const result = await this.executeCommand<DataListParams, DataListResult<RoomEntity>>(DATA_COMMANDS.LIST, {
          collection: 'rooms',
          filter: { uniqueId: roomIdOrName },
          limit: 1
        });

        if (result.success && result.items?.[0]) {
          room = result.items[0];
          roomId = room.id as UUID;
          roomName = room.displayName || room.name || roomIdOrName;
          roomUniqueId = room.uniqueId || roomIdOrName;
        } else {
          // Try as UUID directly
          roomId = roomIdOrName as UUID;
        }
      }

      // SAME ROOM: Refresh messages instead of skipping entirely
      // New messages may have arrived while viewing other content
      if (roomId === this.currentRoomId) {
        verbose() && console.log(`📨 ChatWidget: switchToRoom same room, refreshing`);
        await this.scroller?.refresh();
        this.updateHeader();
        return;
      }

      // Update state - track BOTH UUID and uniqueId for guard comparisons
      this.currentRoomId = roomId;
      this.currentRoomName = roomName;
      this.currentRoomUniqueId = roomUniqueId;

      // Reset counters for new room
      this.totalMessageCount = 0;
      this.loadedMessageCount = 0;

      // Clear AI status indicators for previous room
      this.aiStatusIndicator.clearAll();

      // Update header immediately
      this.updateHeader();

      // Load room data and refresh messages
      await Promise.all([
        this.loadRoomData(roomId),
        this.scroller?.refresh()
      ]);

      // Update header with correct count
      this.updateHeader();

      // NOTE: Removed PositronWidgetState.emit() - MainWidget handles context
      // Widgets should RECEIVE state, not emit it (avoid cascade)

    } catch (error) {
      console.error('ChatWidget: Failed to switch room:', error);
    }
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/chat/chat-widget/${filename}`;
  }

  // Required by EntityScrollerWidget - render function for individual messages
  protected getRenderFunction(): RenderFn<ChatMessageEntity> {
    return (message: ChatMessageEntity, _context) => {
      const isCurrentUser = message.senderId === DEFAULT_USERS.HUMAN;
      const senderName = message.senderName || 'Unknown';

      // Select adapter based on message content (text, image, video, etc.)
      const adapter = this.adapterRegistry.selectAdapter(message);
      const contentHtml = adapter
        ? adapter.renderMessage(message, DEFAULT_USERS.HUMAN)
        : `<p>${message.content?.text || '(no content)'}</p>`;

      const messageElement = globalThis.document.createElement('div');
      // Show pending messages with lower opacity (optimistic update)
      const postingClass = message.status === 'sending' ? ' posting' : '';
      messageElement.className = `message-row ${isCurrentUser ? 'right' : 'left'}${postingClass}`;
      // CRITICAL: Add entity ID to DOM for testing/debugging (test expects 'message-id')
      messageElement.setAttribute('message-id', message.id);

      // Build message structure with DOM APIs (no innerHTML for static structure)
      const bubble = globalThis.document.createElement('div');
      bubble.className = `message-bubble ${isCurrentUser ? 'current-user' : 'other-user'}`;

      const header = globalThis.document.createElement('div');
      header.className = 'message-header';

      const senderSpan = globalThis.document.createElement('span');
      senderSpan.className = 'sender-name';
      senderSpan.textContent = senderName;

      const timeSpan = globalThis.document.createElement('span');
      timeSpan.className = 'message-time';
      timeSpan.textContent = new Date(message.timestamp).toLocaleString();

      header.appendChild(senderSpan);
      header.appendChild(timeSpan);

      const contentDiv = globalThis.document.createElement('div');
      contentDiv.className = 'message-content';
      // Adapter content uses innerHTML - adapters return HTML strings
      // TODO: Refactor adapters to return DOM elements for full innerHTML elimination
      contentDiv.innerHTML = contentHtml;

      bubble.appendChild(header);
      bubble.appendChild(contentDiv);
      messageElement.appendChild(bubble);

      // Initialize adapter content loading (e.g., image load handlers)
      if (adapter && adapter.handleContentLoading) {
        adapter.handleContentLoading(contentDiv).catch((err) => {
          console.error('Failed to handle content loading:', err);
        });
      }

      return messageElement;
    };
  }

  // Required by EntityScrollerWidget - load function using data/list command
  protected getLoadFunction(): LoadFn<ChatMessageEntity> {
    return async (cursor, limit) => {
      // CRITICAL: If no roomId is selected, return 0 messages immediately
      if (!this.currentRoomId) {
        return {
          items: [],
          hasMore: false,
          nextCursor: undefined
        };
      }

      // DEBUG logs removed - were causing WebSocket spam

      // CRITICAL: The filter MUST be applied at the database level, not client-side
      // This ensures we only get messages for THIS room, with proper paging/cursors
      // Load NEWEST messages first (DESC) so recent messages appear after refresh
      // EntityScroller + CSS handle display order based on SCROLLER_PRESETS.CHAT direction
      // CRITICAL: backend='server' ensures we always fetch fresh data, not stale localStorage cache
      const result = await Commands.execute<DataListParams, DataListResult<ChatMessageEntity>>(DATA_COMMANDS.LIST, {
        collection: ChatMessageEntity.collection,
        filter: {
          roomId: this.currentRoomId,
          status: 'sent' // Only show successfully sent messages
        },
        orderBy: [{ field: 'timestamp', direction: 'desc' }], // Load NEWEST first
        limit: limit ?? 30, // Default page size matches SCROLLER_PRESETS.CHAT
        backend: 'stale-while-revalidate', // Show cached instantly, refresh with server data
        ...(cursor && { cursor: { field: 'timestamp', value: cursor, direction: 'before' } }) // 'before' = older than cursor for DESC queries
      });

      if (!result?.success || !result.items) {
        throw new Error(`Failed to load messages for room ${this.currentRoomId}: ${result?.error ?? 'Unknown error'}`);
      }

      // Reduce log spam
      // verbose() && console.log(`🔧 CLAUDE-DEBUG: Database returned ${result.items.length} messages for room "${this.currentRoomId}", total count: ${result.count}`);

      // Store total count from database (not just loaded items)
      this.totalMessageCount = result.count ?? result.items.length;

      // Only filter out empty messages, NOT by roomId (database already did that)
      const validMessages = result.items.filter(msg => msg.content?.text?.trim());

      // Populate EntityCacheService with loaded messages (Positronic pattern)
      // This makes the cache the single source of truth for entity data
      entityCache.populate<ChatMessageEntity>(ChatMessageEntity.collection, validMessages);

      // Calculate if there are more messages to load
      // Update running total of loaded messages
      if (!cursor) {
        // First load - reset counter
        this.loadedMessageCount = validMessages.length;
      } else {
        // Subsequent load - add to counter
        this.loadedMessageCount += validMessages.length;
      }

      // Use backend's total count to determine if more records exist
      const hasMoreMessages = this.loadedMessageCount < this.totalMessageCount;

      // For cursor-based pagination, use the oldest message's timestamp as next cursor
      let nextCursor: string | undefined;
      if (hasMoreMessages && validMessages.length > 0) {
        // Since we load DESC (newest first), the LAST item is the oldest
        const oldestMessage = validMessages[validMessages.length - 1];
        nextCursor = oldestMessage?.timestamp?.toString();
      }

      // PAGINATION log removed - too verbose for hot path

      return {
        items: validMessages,
        hasMore: hasMoreMessages,
        nextCursor: nextCursor
      };
    };
  }

  // Required by EntityScrollerWidget
  protected getScrollerPreset(): ScrollerConfig {
    // Enable auto-scroll only when user is at bottom
    return {
      ...SCROLLER_PRESETS.CHAT,
      autoScroll: {
        enabled: true, // Auto-scroll when new messages arrive
        threshold: 200, // Forgiving threshold - tolerates error message scrolls while preventing aggressive yanking
        behavior: 'smooth' as const
      }
    };
  }

  // Required by EntityScrollerWidget
  protected getContainerSelector(): string {
    return '.messages-container';
  }

  // Required by EntityScrollerWidget
  protected getEntityCollection(): string {
    return ChatMessageEntity.collection;
  }

  // Override to show total database count instead of just loaded entities
  protected override getEntityCount(): number {
    return this.totalMessageCount;
  }

  // CRITICAL: Override filtering hooks to only accept messages from current room
  protected override shouldAddEntity(entity: ChatMessageEntity): boolean {
    // Only add messages that belong to the current room
    const shouldAdd = !!(this.currentRoomId && entity.roomId === this.currentRoomId);

    if (shouldAdd) {
      // 🚀 OPTIMISTIC UPDATE: Remove temp message when real one arrives
      // Check if this is from the human user (Joel) and we have pending temp messages
      if (entity.senderType === 'human' && this._pendingMessageTempIds.size > 0) {
        // Remove the oldest pending temp message (FIFO order)
        const tempId = this._pendingMessageTempIds.values().next().value;
        if (tempId) {
          this._pendingMessageTempIds.delete(tempId);
          this.scroller?.remove(tempId);
        }
      }

      // CRITICAL: Increment total count when new message is accepted via events
      this.totalMessageCount++;
      this.loadedMessageCount++;
    }

    return shouldAdd;
  }

  protected override shouldUpdateEntity(id: string, entity: ChatMessageEntity): boolean {
    // Only update messages that belong to the current room
    const shouldUpdate = !!(this.currentRoomId && entity.roomId === this.currentRoomId);
    return shouldUpdate;
  }

  protected override shouldRemoveEntity(id: string): boolean {
    // For deletions we only have the ID, not the full entity
    // We need to check if the message exists in our current scroller (which means it belongs to current room)
    const currentEntities = this.scroller?.entities() || [];
    const shouldRemove = currentEntities.some(entity => entity.id === id);
    return shouldRemove;
  }

  // Required by EntityScrollerWidget - title for header
  protected getEntityTitle(_entity?: ChatMessageEntity): string {
    return this.currentRoomName; // Show the current room name
  }


  // Override to add room selection handler AND enable ChatMessage events
  protected override async onWidgetInitialize(): Promise<void> {
    // IMPORTANT: Call parent to setup ChatMessage event subscriptions for real-time updates
    await super.onWidgetInitialize();

    // === STEP 1: Determine widget mode (pinned vs. dynamic) ===
    const roomAttr = this.getAttribute('room');
    this._isPinnedWidget = !!roomAttr;

    if (this._isPinnedWidget && roomAttr) {
      // Pinned widget (e.g., right panel) - load specific room, ignore events
      await this.switchToRoom(roomAttr);
      this._isActiveContent = true; // Pinned widgets are always "active"
    } else {
      // Dynamic widget - use pageState as single source of truth
      await this.initializeDynamicWidget();
    }

    // NOTE: Step 2 removed - switchToRoom() already calls loadRoomData()
    // Having it here caused duplicate DB queries (room data loaded twice)

    // === STEP 2: Setup event subscriptions (with cleanup tracking) ===
    this.setupContentEventSubscriptions();
    this.setupAIEventSubscriptions();
    this.setupLearningEventSubscriptions();

    // === STEP 4: Subscribe to page state changes (React-like pattern) ===
    if (!this._isPinnedWidget) {
      this.subscribeToPageState((newState) => {
        const isNowActive = newState?.contentType === 'chat';
        this.setActiveContent(isNowActive, 'pageState change');

        // If chat became active and room changed, switch to it
        if (isNowActive && newState?.entityId && newState.entityId !== this.currentRoomId) {
          this.handleRoomSwitch(newState.entityId, newState.resolved?.displayName, 'pageState');
        }
      });
    }

    // 🧠 REACTIVE WIDGET PATTERN: Subscribe to UserProfileWidget events
    this.positronUnsubscribe = PositronWidgetState.subscribeToWidget('profile', 'status:changed', (data) => {
      this.handleProfileStatusChange(data as { userId: string; status: string; displayName: string });
    });

    // === STEP 5: Setup signal watchers for reactive UI updates ===
    this.setupSignalWatchers();
  }

  /**
   * Setup signal watchers for reactive UI updates (React-like pattern)
   * These automatically update the UI when signal state changes
   */
  private setupSignalWatchers(): void {
    // Watch totalMessageCount - auto-update header when count changes
    const countWatcher = watch(
      this._signals.getSignal('totalMessageCount'),
      () => this.updateHeader()
    );
    this._signalDisposers.push(countWatcher);

    // Watch roomName - auto-update header when room changes
    const roomWatcher = watch(
      this._signals.getSignal('roomName'),
      () => this.updateHeader()
    );
    this._signalDisposers.push(roomWatcher);
  }

  /**
   * Initialize a dynamic (non-pinned) widget from pageState or fallbacks
   */
  private async initializeDynamicWidget(): Promise<void> {
    // Determine initial room from multiple sources (priority order):
    // 1. pageState (SINGLE SOURCE OF TRUTH - set by MainWidget before creating widget)
    // 2. entity-id attribute (legacy fallback)
    // 3. UserState.contentState (legacy fallback)
    // 4. Default to General

    if (this.pageState?.contentType === 'chat' && this.pageState.entityId) {
      verbose() && console.log(`📨 ChatWidget: Using pageState room="${this.pageState.entityId}"`);
      await this.switchToRoom(this.pageState.entityId);
      this._isActiveContent = true;
    } else {
      const entityIdAttr = this.getAttribute('entity-id') || this.getAttribute('data-entity-id');
      if (entityIdAttr) {
        verbose() && console.log(`📨 ChatWidget: Using entity-id="${entityIdAttr}" from attribute (legacy)`);
        await this.switchToRoom(entityIdAttr);
        this._isActiveContent = true;
      } else {
        await this.loadCurrentRoomFromUserState();
        // Assume active if we loaded from user state
        this._isActiveContent = true;
      }
    }
  }

  /**
   * Setup content-related event subscriptions (content:opened, content:switched)
   * CONSOLIDATED: All events use the same handleRoomSwitch method
   * NOTE: ROOM_SELECTED removed - pageState subscription handles room changes
   */
  private setupContentEventSubscriptions(): void {
    // Skip all content events for pinned widgets
    if (this._isPinnedWidget) {
      return;
    }

    // content:opened - from content/open command
    this.subscribeWithCleanup('content:opened', (data: { contentType: string; entityId: string; title: string }) => {
      if (data.contentType === 'chat' && data.entityId) {
        this.setActiveContent(true, 'content:opened');
        this.handleRoomSwitch(data.entityId, data.title, 'content:opened');
      }
    });

    // content:switched - from tab clicks
    this.subscribeWithCleanup('content:switched', (data: { contentType?: string; entityId?: string; title?: string }) => {
      if (data.contentType === 'chat' && data.entityId) {
        this.setActiveContent(true, 'content:switched');
        this.handleRoomSwitch(data.entityId, data.title, 'content:switched');
      } else if (data.contentType && data.contentType !== 'chat') {
        // User switched to non-chat content - we're no longer active
        this.setActiveContent(false, `switched to ${data.contentType}`);
      }
    });
  }

  /**
   * Setup AI decision event subscriptions (thinking, generating, posted, etc.)
   */
  private setupAIEventSubscriptions(): void {
    const aiEventHandler = (event: string, handler: (data: any) => void) => {
      this.subscribeWithCleanup(event, (data: any) => {
        // Only process events for current room
        if (data.roomId === this.currentRoomId) {
          handler(data);
          this.updateHeader();
        }
      });
    };

    aiEventHandler(AI_DECISION_EVENTS.EVALUATING, (data) => this.aiStatusIndicator.onEvaluating(data));
    aiEventHandler(AI_DECISION_EVENTS.DECIDED_RESPOND, (data) => this.aiStatusIndicator.onDecidedRespond(data));
    aiEventHandler(AI_DECISION_EVENTS.DECIDED_SILENT, (data) => this.aiStatusIndicator.onDecidedSilent(data));
    aiEventHandler(AI_DECISION_EVENTS.GENERATING, (data) => this.aiStatusIndicator.onGenerating(data));
    aiEventHandler(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, (data) => this.aiStatusIndicator.onCheckingRedundancy(data));
    aiEventHandler(AI_DECISION_EVENTS.ERROR, (data) => this.aiStatusIndicator.onError(data));

    // POSTED event - AI finished responding
    this.subscribeWithCleanup(AI_DECISION_EVENTS.POSTED, (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.aiStatusIndicator.onPosted(data);
        this.updateHeader();
      }
    });
  }

  /**
   * Setup AI learning event subscriptions (training indicators)
   */
  private setupLearningEventSubscriptions(): void {
    this.subscribeWithCleanup(AI_LEARNING_EVENTS.TRAINING_STARTED, (data: any) => {
      this.addLearningBorder(data.personaName);
    });

    this.subscribeWithCleanup(AI_LEARNING_EVENTS.TRAINING_COMPLETE, () => {
      this.removeLearningBorder();
    });

    this.subscribeWithCleanup(AI_LEARNING_EVENTS.TRAINING_ERROR, () => {
      this.removeLearningBorder();
    });
  }

  /**
   * Handle profile status changes from UserProfileWidget
   *
   * REACTIVE WIDGET PATTERN: This enables ChatWidget to react to UserProfileWidget
   * events without polling. When a user's status changes, update our member display.
   */
  private handleProfileStatusChange(data: { userId: string; status: string; displayName: string }): void {
    const profileUserId = data.userId as UUID;

    // Check if this user is a member of our current room
    if (!this.roomMembers.has(profileUserId)) {
      return;
    }

    // Debounce rapid updates
    if (this.positronUpdateDebounce) {
      clearTimeout(this.positronUpdateDebounce);
    }

    this.positronUpdateDebounce = setTimeout(() => {
      const member = this.roomMembers.get(profileUserId);
      if (member && member.status !== data.status) {
        verbose() && console.log(`🧠 ChatWidget: Profile event - ${member.displayName} status: ${member.status} → ${data.status}`);

        // Update the member's status
        member.status = data.status as UserEntity['status'];

        // Refresh the header to show updated member status
        this.updateHeader();
      }

      this.positronUpdateDebounce = undefined;
    }, 100) as unknown as number; // 100ms debounce
  }

  /**
   * Cleanup when widget is disconnected from DOM
   * CRITICAL: Must clean up all event subscriptions to prevent memory leaks
   */
  async disconnectedCallback(): Promise<void> {

    // Clean up ALL tracked event subscriptions (React-like cleanup pattern)
    for (const unsubscribe of this._eventUnsubscribers) {
      try {
        unsubscribe();
      } catch (e) {
        // Ignore errors during cleanup
      }
    }
    this._eventUnsubscribers = [];

    // Clean up signal watch/effect subscriptions
    for (const dispose of this._signalDisposers) {
      try {
        dispose();
      } catch (e) {
        // Ignore errors during cleanup
      }
    }
    this._signalDisposers = [];

    // Dispose the entire signal store (cleans up all internal effects)
    this._signals.dispose();

    // Clean up Positron subscription
    if (this.positronUnsubscribe) {
      this.positronUnsubscribe();
      this.positronUnsubscribe = undefined;
    }

    // Clear any pending debounce
    if (this.positronUpdateDebounce) {
      clearTimeout(this.positronUpdateDebounce);
      this.positronUpdateDebounce = undefined;
    }

    // Clear any pending header update
    if (this.headerUpdateTimeout) {
      clearTimeout(this.headerUpdateTimeout);
      this.headerUpdateTimeout = undefined;
    }

    // Detach event delegator to clean up the single listener
    this.eventDelegator.detach();

    // Call parent cleanup
    await super.disconnectedCallback();
  }

  /**
   * Add learning border to chat widget
   */
  private addLearningBorder(personaName: string): void {
    const container = this.shadowRoot?.querySelector('.entity-list-container') as HTMLElement;
    if (container) {
      container.classList.add('learning-active');
      container.dataset.learningPersona = personaName;
      verbose() && console.log(`🧬 ChatWidget: Added learning border for ${personaName}`);
    }
  }

  /**
   * Remove learning border from chat widget
   */
  private removeLearningBorder(): void {
    const container = this.shadowRoot?.querySelector('.entity-list-container') as HTMLElement;
    if (container) {
      container.classList.remove('learning-active');
      delete container.dataset.learningPersona;
      verbose() && console.log(`🧬 ChatWidget: Removed learning border`);
    }
  }

  // Note: We receive ALL ChatMessage events, but EntityScroller will filter them
  // during refresh() calls using our room-specific getLoadFunction()
  // This is inefficient but works with current event system limitations

  // Override template to include AI status container and message input footer
  protected renderTemplate(): string {
    return `
      <div class="entity-list-container">
        ${this.renderHeader()}

        <!-- AI Status Indicators Container (sticky above messages) -->
        <div class="ai-status-container" id="aiStatusContainer">
          <div class="ai-status-summary" id="aiStatusSummary"></div>
        </div>

        <div class="entity-list-body messages-container">
          <!-- EntityScroller will populate this container -->
        </div>

        ${this.renderFooter()}
      </div>
    `;
  }

  // Custom footer with message input
  protected renderFooter(): string {
    return `
      <div class="input-container">
        <textarea class="message-input" id="messageInput" placeholder="Type a message... (or drag & drop files)" rows="1"></textarea>
        <button class="send-button" id="sendButton">Send</button>
      </div>
    `;
  }

  // Override to setup message composer after EntityScroller initialization
  protected override async renderWidget(): Promise<void> {
    await super.renderWidget();

    // Inject ALL adapter CSS into shadow DOM (once per widget, not per message)
    if (this.shadowRoot) {
      AbstractMessageAdapter.injectAdapterStyles(this.shadowRoot, this.adapterRegistry.getAllAdapters());
    }

    // Cache AI status container and wire up to status indicator
    this.aiStatusContainer = this.shadowRoot?.getElementById('aiStatusContainer') as HTMLElement;
    if (this.aiStatusContainer) {
      this.aiStatusIndicator.setContainer(this.aiStatusContainer);
      // Set initial display state based on errorsHidden flag
      this.aiStatusContainer.style.display = this.errorsHidden ? 'none' : 'block';
      verbose() && console.log(`✅ ChatWidget: AI status container ready`);
    }

    // Setup error toggle handler
    this.setupErrorToggleHandler();

    // Setup member click handlers (for initial HTML-rendered chips)
    this.setupMemberClickHandlers();

    // Cache input element after DOM is rendered
    this.messageInput = this.shadowRoot?.getElementById('messageInput') as HTMLTextAreaElement;
    this.setupMessageInputHandlers();

    // Add drag-and-drop to entire chat widget
    const container = this.shadowRoot?.querySelector('.entity-list-container') as HTMLElement;
    if (container) {
      container.addEventListener('dragover', (e) => this.handleDragOver(e));
      container.addEventListener('dragleave', (e) => this.handleDragLeave(e));
      container.addEventListener('drop', (e) => this.handleDrop(e));

      // Apply compact mode if set via attribute
      this.updateCompactMode();

      // === EVENT DELEGATION FOR MEMORY-EFFICIENT MESSAGE INTERACTIONS ===
      // Attach single listener to container instead of per-element listeners
      this.eventDelegator.attach(container);

      // Register ImageMessageAdapter action handlers
      this.eventDelegator.onAction('image-fullscreen', (target) => ImageMessageAdapter.handleFullscreen(target));
      this.eventDelegator.onAction('image-download', (target) => ImageMessageAdapter.handleDownload(target));
      this.eventDelegator.onAction('image-ai-describe', (target) => ImageMessageAdapter.handleAIDescribe(target));
      this.eventDelegator.onAction('image-retry', (target) => ImageMessageAdapter.handleRetry(target));

      // Register URLCardAdapter action handlers
      this.eventDelegator.onAction('url-card-click', (target, event) => URLCardAdapter.handleCardClick(target, event));
      this.eventDelegator.onAction('url-open-external', (target) => URLCardAdapter.handleOpenExternal(target));
      this.eventDelegator.onAction('url-ai-summarize', (target) => URLCardAdapter.handleAISummarize(target));
      this.eventDelegator.onAction('url-retry-preview', (target) => URLCardAdapter.handleRetryPreview(target));

      verbose() && console.log('✅ ChatWidget: Event delegator attached with action handlers');
    }
  }


  /**
   * Load room data and member information
   */
  private async loadRoomData(roomId: UUID): Promise<void> {
    console.log(`🔍 ChatWidget.loadRoomData: Loading room ${roomId}`);
    try {
      // Load room entity - use server backend to ensure we get full data with members
      const roomResult = await Commands.execute<DataReadParams, DataReadResult<RoomEntity>>(DATA_COMMANDS.READ, {
        collection: RoomEntity.collection,
        id: roomId,
        backend: 'server'
      });

      console.log(`🔍 ChatWidget.loadRoomData: Result success=${roomResult?.success}, hasData=${!!roomResult?.data}, memberCount=${roomResult?.data?.members?.length ?? 0}`);

      if (!roomResult?.success || !roomResult.data) {
        console.error(`❌ ChatWidget: Failed to load room data for ${roomId}`);
        return;
      }

      this.currentRoom = roomResult.data;

      // Load user details for each member
      await this.loadRoomMembers();
    } catch (error) {
      console.error(`❌ ChatWidget: Error loading room data:`, error);
    }
  }

  /**
   * Load user entities for all room members in a single batch query
   * Optimized: Uses data/list with id filter instead of N individual reads
   */
  private async loadRoomMembers(): Promise<void> {
    console.log(`🔍 ChatWidget.loadRoomMembers: currentRoom=${!!this.currentRoom}, memberCount=${this.currentRoom?.members?.length ?? 0}`);
    if (!this.currentRoom || this.currentRoom.members.length === 0) return;

    this.roomMembers.clear();

    // Batch load all members in ONE query (was N+1 queries before)
    const memberIds = this.currentRoom.members.map(m => m.userId);
    console.log(`🔍 ChatWidget.loadRoomMembers: Loading ${memberIds.length} members:`, memberIds.slice(0, 3));

    try {
      // Uses MongoDB-style $in operator for batch ID lookup
      // Must use server backend - localStorage doesn't support $in operator
      const result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(DATA_COMMANDS.LIST, {
        collection: UserEntity.collection,
        filter: { id: { $in: memberIds } },
        limit: memberIds.length,
        backend: 'server'
      });

      console.log(`🔍 ChatWidget.loadRoomMembers: Result success=${result?.success}, itemCount=${result?.items?.length ?? 0}`);
      if (result?.success && result.items) {
        for (const user of result.items) {
          this.roomMembers.set(user.id as UUID, user);
        }
        console.log(`✅ ChatWidget.loadRoomMembers: Loaded ${this.roomMembers.size} members`);
      }
    } catch (error) {
      console.warn(`⚠️ ChatWidget: Failed to batch load members:`, error);
    }
  }

  /**
   * Load current room from UserState's currentContentItem
   * This is called on init to handle tab switching - when a new ChatWidget is created
   * after tab click, it needs to know which room to display (not just default to General)
   */
  private async loadCurrentRoomFromUserState(): Promise<void> {
    try {
      // Get current user's ID from session
      const sessionResult = await Commands.execute<any, any>('session/get-user', {});
      if (!sessionResult?.success || !sessionResult.userId) {
        verbose() && console.log('📨 ChatWidget: No user session, using default General room');
        return;
      }

      // Query UserState to get current content item
      const listResult = await Commands.execute<DataListParams, DataListResult<any>>(DATA_COMMANDS.LIST, {
        collection: 'user_states',
        filter: { userId: sessionResult.userId },
        limit: 1
      });

      if (!listResult?.success || !listResult.items?.length) {
        verbose() && console.log('📨 ChatWidget: No UserState found, using default General room');
        return;
      }

      const userState = listResult.items[0];
      const currentItemId = userState.contentState?.currentItemId;
      const openItems = userState.contentState?.openItems || [];

      // Find the current content item
      const currentItem = openItems.find((item: any) => item.id === currentItemId);
      if (!currentItem) {
        verbose() && console.log('📨 ChatWidget: No current content item, using default General room');
        return;
      }

      // Only use if it's a chat type with an entityId (room ID)
      if (currentItem.type === 'chat' && currentItem.entityId) {
        verbose() && console.log(`📨 ChatWidget: Loading room from UserState: "${currentItem.title}" (${currentItem.entityId})`);
        this.currentRoomId = currentItem.entityId as UUID;
        this.currentRoomName = currentItem.title || 'Chat';
      } else {
        verbose() && console.log(`📨 ChatWidget: Current content is "${currentItem.type}", not a chat room`);
      }
    } catch (error) {
      console.warn('⚠️ ChatWidget: Error loading from UserState, using default:', error);
    }
  }

  /**
   * Override renderHeader to display room members
   */
  protected override renderHeader(): string {
    // Get member display (avatars/names)
    const memberDisplay = this.renderMemberList();

    // Use room topic if available (more descriptive), otherwise fall back to description or name
    const headerText = this.currentRoom?.topic
      || this.currentRoom?.description
      || this.currentRoomName;

    // Error count for toggle button indicator
    const errorCount = this.aiStatusIndicator.getErrorCount();

    return `
      <div class="entity-list-header">
        <div class="header-top">
          <span class="header-title">${headerText}</span>
          <button
            class="error-toggle ${!this.errorsHidden ? 'pressed' : ''}"
            id="errorToggle"
            title="${this.errorsHidden ? 'Show errors' : 'Hide errors'} ${errorCount > 0 ? `(${errorCount})` : ''}"
          >
            Errors 🗑️${errorCount > 0 ? ` (${errorCount})` : ''}
          </button>
          <span class="list-count">${this.getEntityCount()}</span>
        </div>
        <div class="header-members">
          ${memberDisplay}
        </div>
      </div>
    `;
  }

  /**
   * Render the list of room members
   * Shows role icon + AI status emoji (if active) + name
   */
  private renderMemberList(): string {
    if (!this.currentRoom || this.roomMembers.size === 0) {
      return '<span class="no-members">Loading members...</span>';
    }

    const memberElements = Array.from(this.roomMembers.values())
      .map(user => {
        const displayName = user.displayName || user.username || 'Unknown';
        const role = this.getMemberRole(user.id);
        const roleIcon = role === 'owner' ? '👑' : role === 'admin' ? '⭐' : '';

        // Get AI status emoji (thinking, generating, etc.) for subtle indication
        const statusEmoji = this.aiStatusIndicator.getStatusEmoji(user.id) || '';

        // Check status type for styling and click behavior
        const hasError = statusEmoji === '❌' || statusEmoji === '💸' || statusEmoji === '⏳';
        const hasStatus = statusEmoji !== '';
        const clickableClass = hasError ? 'clickable-error' : hasStatus ? 'clickable-status' : '';
        const clickHint = hasError ? ' - Click to view error' : hasStatus ? ' - Click to view status' : '';

        return `
          <div class="member-chip ${clickableClass}"
               title="${displayName} (${role})${clickHint}"
               data-persona-id="${user.id}"
               data-has-error="${hasError}"
               data-has-status="${hasStatus}">
            ${roleIcon}
            <span class="member-name">${displayName}</span>
            ${statusEmoji ? `<span class="member-status">${statusEmoji}</span>` : ''}
          </div>
        `;
      })
      .join('');

    return `<div class="members-list">${memberElements}</div>`;
  }

  /**
   * Get the role of a member by userId
   */
  private getMemberRole(userId: UUID): string {
    if (!this.currentRoom) return 'member';
    const member = this.currentRoom.members.find(m => m.userId === userId);
    return member?.role || 'member';
  }

  /**
   * Update the header with current room and member information
   * Debounced to prevent rebuilding on every event
   *
   * REFACTORED: Uses targeted DOM updates instead of innerHTML replacement
   * This preserves event handlers and is more efficient
   */
  private updateHeader(): void {
    // Clear any pending update
    if (this.headerUpdateTimeout) {
      clearTimeout(this.headerUpdateTimeout);
    }

    // Schedule update for next frame
    this.headerUpdateTimeout = setTimeout(() => {
      this.updateHeaderElements();
      // Update the compact status summary line
      this.updateStatusSummary();
      this.headerUpdateTimeout = undefined;
    }, 0) as unknown as number;
  }

  /**
   * Targeted DOM updates for header elements
   * Avoids innerHTML replacement - preserves handlers, more efficient
   */
  private updateHeaderElements(): void {
    const headerElement = this.shadowRoot?.querySelector('.entity-list-header');
    if (!headerElement) return;

    // Update title text
    const titleElement = headerElement.querySelector('.header-title');
    if (titleElement) {
      const headerText = this.currentRoom?.topic
        || this.currentRoom?.description
        || this.currentRoomName;
      titleElement.textContent = headerText;
    }

    // Update message count
    const countElement = headerElement.querySelector('.list-count');
    if (countElement) {
      countElement.textContent = String(this.getEntityCount());
    }

    // Update error toggle button
    const errorToggle = headerElement.querySelector('#errorToggle') as HTMLButtonElement;
    if (errorToggle) {
      const errorCount = this.aiStatusIndicator.getErrorCount();
      errorToggle.textContent = `Errors 🗑️${errorCount > 0 ? ` (${errorCount})` : ''}`;
      errorToggle.title = `${this.errorsHidden ? 'Show errors' : 'Hide errors'} ${errorCount > 0 ? `(${errorCount})` : ''}`;
    }

    // Update members list (rebuild this section only)
    const membersContainer = headerElement.querySelector('.header-members');
    if (membersContainer) {
      this.updateMembersList(membersContainer as HTMLElement);
    }
  }

  /**
   * Update members list with targeted DOM manipulation
   * Creates/updates member chips without full innerHTML replacement
   */
  private updateMembersList(container: HTMLElement): void {
    // Get or create the members-list div
    let membersList = container.querySelector('.members-list') as HTMLElement;

    if (!this.currentRoom || this.roomMembers.size === 0) {
      // Show loading state - use DOM manipulation instead of innerHTML
      if (!membersList) {
        const existingContent = container.querySelector('.no-members');
        if (!existingContent) {
          container.textContent = ''; // Clear any existing content
          const loadingSpan = document.createElement('span');
          loadingSpan.className = 'no-members';
          loadingSpan.textContent = 'Loading members...';
          container.appendChild(loadingSpan);
        }
      }
      return;
    }

    // Create members-list if it doesn't exist
    if (!membersList) {
      // Clear container using DOM methods
      while (container.firstChild) {
        container.removeChild(container.firstChild);
      }
      membersList = document.createElement('div');
      membersList.className = 'members-list';
      container.appendChild(membersList);
    }

    // Build a map of existing chips by persona ID for efficient updates
    const existingChips = new Map<UUID, HTMLElement>();
    membersList.querySelectorAll('.member-chip').forEach((chip) => {
      const personaId = chip.getAttribute('data-persona-id') as UUID;
      if (personaId) {
        existingChips.set(personaId, chip as HTMLElement);
      }
    });

    // Track which IDs we've processed (to remove stale ones)
    const processedIds = new Set<UUID>();

    // Update or create chips for each member
    for (const user of this.roomMembers.values()) {
      const data = this.buildMemberChipData(user);
      processedIds.add(data.personaId);

      let chip = existingChips.get(data.personaId);

      if (chip) {
        this.updateMemberChip(chip, data);
      } else {
        chip = this.createMemberChip(data);
        membersList.appendChild(chip);
      }
    }

    // Remove chips for members no longer in the room
    for (const [personaId, chip] of existingChips) {
      if (!processedIds.has(personaId)) {
        chip.remove();
      }
    }
  }

  /**
   * Build MemberChipData from a UserEntity
   */
  private buildMemberChipData(user: UserEntity): MemberChipData {
    const role = this.getMemberRole(user.id);
    const statusEmoji = this.aiStatusIndicator.getStatusEmoji(user.id) || '';

    return {
      personaId: user.id,
      displayName: user.displayName || 'Unknown',
      role,
      roleIcon: role === 'owner' ? '👑' : role === 'admin' ? '⭐' : '',
      statusEmoji,
      hasError: statusEmoji === '❌' || statusEmoji === '💸' || statusEmoji === '⏳',
      hasStatus: statusEmoji !== ''
    };
  }

  /**
   * Create a member chip element from MemberChipData
   */
  private createMemberChip(data: MemberChipData): HTMLElement {
    const chip = document.createElement('div');
    chip.className = `member-chip${data.hasError ? ' clickable-error' : data.hasStatus ? ' clickable-status' : ''}`;
    chip.setAttribute('data-persona-id', data.personaId);
    chip.setAttribute('data-has-error', String(data.hasError));
    chip.setAttribute('data-has-status', String(data.hasStatus));

    const clickHint = data.hasError ? ' - Click to view error' : data.hasStatus ? ' - Click to view status' : '';
    chip.title = `${data.displayName} (${data.role})${clickHint}`;

    // Build chip content
    if (data.roleIcon) {
      chip.appendChild(document.createTextNode(data.roleIcon));
    }

    const nameSpan = document.createElement('span');
    nameSpan.className = 'member-name';
    nameSpan.textContent = data.displayName;
    chip.appendChild(nameSpan);

    if (data.statusEmoji) {
      const statusSpan = document.createElement('span');
      statusSpan.className = 'member-status';
      statusSpan.textContent = data.statusEmoji;
      chip.appendChild(statusSpan);
    }

    // Add click handler for chips with status
    if (data.hasStatus) {
      chip.addEventListener('click', () => {
        verbose() && console.log(`🔍 ChatWidget: Clicked persona ${data.personaId} (error=${data.hasError})`);
        this.toggleErrorPanel(true);

        if (this.aiStatusContainer) {
          const statusElement = this.aiStatusContainer.querySelector(`[data-persona-id="${data.personaId}"]`);
          if (statusElement) {
            statusElement.classList.add('flash-highlight');
            setTimeout(() => statusElement.classList.remove('flash-highlight'), 1000);
          }
        }
      });
    }

    return chip;
  }

  /**
   * Update an existing member chip element from MemberChipData
   */
  private updateMemberChip(chip: HTMLElement, data: MemberChipData): void {
    // Update classes
    chip.className = `member-chip${data.hasError ? ' clickable-error' : data.hasStatus ? ' clickable-status' : ''}`;
    chip.setAttribute('data-has-error', String(data.hasError));
    chip.setAttribute('data-has-status', String(data.hasStatus));

    const clickHint = data.hasError ? ' - Click to view error' : data.hasStatus ? ' - Click to view status' : '';
    chip.title = `${data.displayName} (${data.role})${clickHint}`;

    // Update name
    const nameSpan = chip.querySelector('.member-name');
    if (nameSpan) {
      nameSpan.textContent = data.displayName;
    }

    // Update or create status span
    let statusSpan = chip.querySelector('.member-status') as HTMLElement;
    if (data.statusEmoji) {
      if (!statusSpan) {
        statusSpan = document.createElement('span');
        statusSpan.className = 'member-status';
        chip.appendChild(statusSpan);
      }
      statusSpan.textContent = data.statusEmoji;
    } else if (statusSpan) {
      statusSpan.remove();
    }
  }

  /**
   * Setup click handler for error toggle button
   */
  private setupErrorToggleHandler(): void {
    const toggleButton = this.shadowRoot?.getElementById('errorToggle');
    if (!toggleButton) return;

    toggleButton.addEventListener('click', () => {
      this.toggleErrorPanel();
    });
  }

  /**
   * Toggle the error panel visibility (shared logic for button and member clicks)
   */
  private toggleErrorPanel(forceShow: boolean = false): void {
    const toggleButton = this.shadowRoot?.getElementById('errorToggle');

    // If forceShow is true, always show; otherwise toggle
    if (forceShow && !this.errorsHidden) {
      return; // Already showing, nothing to do
    }

    this.errorsHidden = forceShow ? false : !this.errorsHidden;

    // Update container visibility
    if (this.aiStatusContainer) {
      this.aiStatusContainer.style.display = this.errorsHidden ? 'none' : 'block';
    }

    // Update button visual state
    if (toggleButton) {
      if (this.errorsHidden) {
        toggleButton.classList.remove('pressed');
        toggleButton.setAttribute('title', `Show errors ${this.aiStatusIndicator.getErrorCount() > 0 ? `(${this.aiStatusIndicator.getErrorCount()})` : ''}`);
      } else {
        toggleButton.classList.add('pressed');
        toggleButton.setAttribute('title', `Hide errors ${this.aiStatusIndicator.getErrorCount() > 0 ? `(${this.aiStatusIndicator.getErrorCount()})` : ''}`);
      }
    }
  }

  /**
   * Setup click handlers for member chips (click-to-diagnose)
   * When a persona with any status is clicked, shows the status panel
   */
  private setupMemberClickHandlers(): void {
    // Handle clicks on personas with ANY status (error or active)
    const memberChips = this.shadowRoot?.querySelectorAll('.member-chip[data-has-status="true"]');
    if (!memberChips) return;

    memberChips.forEach((chip) => {
      chip.addEventListener('click', () => {
        const personaId = chip.getAttribute('data-persona-id');
        const hasError = chip.getAttribute('data-has-error') === 'true';
        verbose() && console.log(`🔍 ChatWidget: Clicked persona ${personaId} (error=${hasError})`);

        // Show the status panel
        this.toggleErrorPanel(true);

        // Highlight the specific persona's status in the container (if visible)
        if (personaId && this.aiStatusContainer) {
          const statusElement = this.aiStatusContainer.querySelector(`[data-persona-id="${personaId}"]`);
          if (statusElement) {
            // Add flash animation for attention
            statusElement.classList.add('flash-highlight');
            setTimeout(() => statusElement.classList.remove('flash-highlight'), 1000);
          }
        }
      });
    });
  }

  /**
   * Update the compact status summary line
   * Shows: "✍️ Helper, Teacher · 🤔 CodeReview · ❌ Fireworks"
   */
  private updateStatusSummary(): void {
    const summaryElement = this.shadowRoot?.getElementById('aiStatusSummary');
    if (!summaryElement) return;

    const summary = this.aiStatusIndicator.getFormattedSummary();
    if (summary) {
      summaryElement.textContent = summary;
      summaryElement.style.display = 'block';
    } else {
      summaryElement.textContent = '';
      summaryElement.style.display = 'none';
    }
  }

  /**
   * Setup handlers for message input (send button, Enter key, auto-grow)
   */
  private setupMessageInputHandlers(): void {
    if (!this.messageInput) return;

    const sendButton = this.shadowRoot?.getElementById('sendButton') as HTMLButtonElement;

    // Send on button click
    sendButton?.addEventListener('click', () => this.sendMessage());

    // Send on Enter key (Shift+Enter for newline)
    this.messageInput.addEventListener('keydown', (e: KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        this.sendMessage();
      }
    });

    // Auto-grow textarea as user types
    this.messageInput.addEventListener('input', () => {
      this.autoGrowTextarea();
    });

    // Initial resize
    this.autoGrowTextarea();
  }

  /**
   * Auto-grow textarea to fit content (max 10 rows)
   */
  private autoGrowTextarea(): void {
    if (!this.messageInput) return;

    // Reset height to calculate new height
    this.messageInput.style.height = 'auto';

    // Calculate new height based on scrollHeight (capped at 10 rows ~= 200px)
    const maxHeight = 200; // ~10 rows
    const newHeight = Math.min(this.messageInput.scrollHeight, maxHeight);

    this.messageInput.style.height = `${newHeight}px`;
  }

  /**
   * Send message with text and any pending attachments
   */
  private async sendMessage(): Promise<void> {
    // Guard against concurrent sends
    if (this.isSending) {
      return;
    }

    this.isSending = true;

    if (!this.messageInput) {
      this.isSending = false;
      return;
    }

    const text = this.messageInput.value.trim();

    // Must have either text or attachments
    if (!text && this.pendingAttachments.length === 0) {
      this.isSending = false;
      return;
    }

    // Can't send message without a room selected
    if (!this.currentRoomId) {
      console.warn('Cannot send message: no room selected');
      this.isSending = false;
      return;
    }

    // Create message entity
    const messageEntity = new ChatMessageEntity();
    messageEntity.roomId = this.currentRoomId;
    messageEntity.senderId = DEFAULT_USERS.HUMAN as UUID;
    messageEntity.senderName = 'Joel';
    messageEntity.senderType = 'human';
    messageEntity.content = {
      text,
      media: this.pendingAttachments.length > 0 ? this.pendingAttachments : undefined
    };
    messageEntity.status = 'sent';
    messageEntity.priority = 'normal';
    messageEntity.timestamp = new Date();
    messageEntity.reactions = [];

    // 🚀 OPTIMISTIC UPDATE: Show message immediately with "sending" state
    const tempId = `temp-${Date.now()}-${Math.random().toString(36).substring(2, 8)}` as UUID;
    messageEntity.id = tempId;
    messageEntity.status = 'sending';

    // Add to scroller immediately - CSS will show pending state (lower opacity)
    if (this.scroller) {
      this.scroller.add(messageEntity);
      this.scroller.scrollToEnd();
    }

    // Track temp ID so we can remove it when real message arrives
    this._pendingMessageTempIds.add(tempId);

    // Clear input
    this.messageInput.value = '';
    const attachmentCount = this.pendingAttachments.length;
    this.pendingAttachments = [];
    this.messageInput.placeholder = 'Type a message... (or drag & drop files)';

    // Reset textarea height to single row
    this.autoGrowTextarea();

    // Use proper chat/send command - it generates proper UUIDs and handles all message setup
    // ARCHITECTURE: Directly replace temp message with real entity from response
    // This is deterministic - no relying on events/FIFO matching
    Commands.execute<ChatSendParams, ChatSendResult>('collaboration/chat/send', {
      message: text,
      room: this.currentRoomId,
      senderId: DEFAULT_USERS.HUMAN as UUID,  // Explicitly send as Joel, not browser session identity
      // Media attachments would need to be file paths for chat/send - TODO: handle properly
    }).then((result) => {
      console.log(`📤 ChatWidget: chat/send response:`, JSON.stringify(result, null, 2));
      console.log(`📤 ChatWidget: Message sent successfully, ID: ${result.messageEntity?.id}, replacing temp: ${tempId}`);

      // CRITICAL: Directly replace temp message with real entity
      // This is the proper pattern for optimistic UI updates
      this._pendingMessageTempIds.delete(tempId);
      if (this.scroller && result.messageEntity) {
        // Remove temp, add real - scroller handles the swap
        this.scroller.remove(tempId);
        this.scroller.add(result.messageEntity);
        this.scroller.scrollToEnd();
      }
    }).catch(error => {
      console.error('Failed to send message:', error);
      // Remove temp message on failure - show error state
      this._pendingMessageTempIds.delete(tempId);
      this.scroller?.remove(tempId);
    });

    // Scroll immediately to give responsive feel
    if (this.scroller) {
      this.scroller.scrollToEnd();
    }

    // Reset sending flag immediately (non-blocking send)
    this.isSending = false;
  }

  // Handle message send from composer widget (legacy - kept for compatibility)
  private async handleMessageSend(event: { text: string; media?: readonly MediaItem[]; replyToId?: string }): Promise<void> {
    const { text, media, replyToId } = event;

    // Can't send message without a room selected
    if (!this.currentRoomId) {
      console.warn('Cannot send message: no room selected');
      return;
    }

    // Create message entity
    const messageEntity = new ChatMessageEntity();
    messageEntity.roomId = this.currentRoomId;
    messageEntity.senderId = DEFAULT_USERS.HUMAN as UUID;
    messageEntity.senderName = 'Joel';
    messageEntity.senderType = 'human';
    messageEntity.content = { text, media };
    messageEntity.status = 'sent';
    messageEntity.priority = 'normal';
    messageEntity.timestamp = new Date();
    messageEntity.reactions = [];

    // Add reply-to field if present
    if (replyToId) {
      messageEntity.replyToId = replyToId as UUID;
    }

    try {
      await Commands.execute<DataCreateParams, DataCreateResult<ChatMessageEntity>>(DATA_COMMANDS.CREATE, {
        collection: ChatMessageEntity.collection,
        data: messageEntity,
        backend: 'server'
      });
      // Message sent - scroll handled by entity events

      // Scroll to bottom after sending OWN message
      if (this.scroller) {
        this.scroller.scrollToEnd();
      }
    } catch (error) {
      console.error('❌ Failed to send message:', error);
    }
  }

  // Drag-and-drop handlers for the entire widget
  private handleDragOver(e: DragEvent): void {
    e.preventDefault();
    e.stopPropagation();
    // Add visual feedback
    const container = this.shadowRoot?.querySelector('.entity-list-container') as HTMLElement;
    if (container) {
      container.classList.add('drag-over');
    }
  }

  private handleDragLeave(e: DragEvent): void {
    e.preventDefault();
    e.stopPropagation();
    // Remove visual feedback
    const container = this.shadowRoot?.querySelector('.entity-list-container') as HTMLElement;
    if (container) {
      container.classList.remove('drag-over');
    }
  }

  private async handleDrop(e: DragEvent): Promise<void> {
    e.preventDefault();
    e.stopPropagation();

    // Remove visual feedback
    const container = this.shadowRoot?.querySelector('.entity-list-container') as HTMLElement;
    if (container) {
      container.classList.remove('drag-over');
    }

    const files = e.dataTransfer?.files;

    if (files && files.length > 0) {
      // Convert all files to MediaItems
      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        try {
          const mediaItem = await this.fileToMediaItem(file);
          this.pendingAttachments.push(mediaItem);
        } catch (error) {
          console.error(`❌ Failed to process ${file.name}:`, error);
        }
      }

      // Update input placeholder to show attachment count and focus input
      if (this.messageInput && this.pendingAttachments.length > 0) {
        this.messageInput.placeholder = `Type a message... (${this.pendingAttachments.length} file${this.pendingAttachments.length > 1 ? 's' : ''} attached)`;
        // Focus input so user can press Enter to send attachments
        this.messageInput.focus();
      }
    }
  }

  /**
   * Convert a File object to a MediaItem with base64 encoding
   */
  private async fileToMediaItem(file: File): Promise<MediaItem> {
    const base64 = await this.readFileAsBase64(file);
    const mediaType = this.inferMediaType(file.type);

    return {
      type: mediaType,
      base64,
      filename: file.name,
      mimeType: file.type,
      size: file.size,
      uploadedAt: Date.now(),
      uploadedBy: DEFAULT_USERS.HUMAN as UUID
    };
  }

  /**
   * Read a file as base64 string
   */
  private readFileAsBase64(file: File): Promise<string> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = reader.result as string;
        // Strip the data URL prefix (e.g., "data:image/png;base64,")
        const base64 = result.split(',')[1];
        resolve(base64);
      };
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
  }

  /**
   * Infer MediaType from MIME type
   */
  private inferMediaType(mimeType: string): MediaType {
    if (mimeType.startsWith('image/')) return 'image';
    if (mimeType.startsWith('audio/')) return 'audio';
    if (mimeType.startsWith('video/')) return 'video';
    if (mimeType.startsWith('application/pdf') ||
        mimeType.startsWith('application/msword') ||
        mimeType.startsWith('application/vnd.openxmlformats')) {
      return 'document';
    }
    return 'file';
  }

  /**
   * Format file size for display
   */
  private formatFileSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

}
