/**
 * Chat Widget - Now extends EntityScrollerWidget for automatic CRUD management
 * Eliminates ~300 lines of manual EntityScroller and CRUD event handling
 */

import { EntityScrollerWidget } from '../../shared/EntityScrollerWidget';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { ChatMessageEntity, type MediaItem, type MediaType } from '../../../system/data/entities/ChatMessageEntity';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { DataReadParams, DataReadResult } from '../../../commands/data/read/shared/DataReadTypes';
import { Commands } from '../../../system/core/shared/Commands';
import { Events } from '../../../system/core/shared/Events';
import { UI_EVENTS } from '../../../system/core/shared/EventConstants';
import { SCROLLER_PRESETS, type RenderFn, type LoadFn, type ScrollerConfig } from '../../shared/EntityScroller';
import { DEFAULT_ROOMS, DEFAULT_USERS } from '../../../system/data/domains/DefaultEntities';
import { AdapterRegistry } from '../adapters/AdapterRegistry';
import { AbstractMessageAdapter } from '../adapters/AbstractMessageAdapter';
import { MessageInputEnhancer } from '../message-input/MessageInputEnhancer';
import { AIStatusIndicator } from './AIStatusIndicator';
import { AI_DECISION_EVENTS } from '../../../system/events/shared/AIDecisionEvents';
import { AI_LEARNING_EVENTS } from '../../../system/events/shared/AILearningEvents';
import { PositronWidgetState } from '../../shared/services/state/PositronWidgetState';
// MessageComposerWidget removed - using inline HTML instead

export class ChatWidget extends EntityScrollerWidget<ChatMessageEntity> {
  private messageInput?: HTMLTextAreaElement;
  private currentRoomId: UUID | null = DEFAULT_ROOMS.GENERAL as UUID; // Default to General room
  private currentRoomName: string = 'General';
  private currentRoom: RoomEntity | null = null;
  private roomMembers: Map<UUID, UserEntity> = new Map(); // Map of userId -> UserEntity
  private totalMessageCount: number = 0; // Total messages in database (not just loaded)
  private loadedMessageCount: number = 0; // Number of messages actually loaded so far
  private adapterRegistry: AdapterRegistry; // Selects adapters per message based on content
  private aiStatusIndicator: AIStatusIndicator; // Manages AI thinking/responding status indicators
  private aiStatusContainer?: HTMLElement; // Container for AI status indicators
  private headerUpdateTimeout?: number; // Debounce timeout for header updates
  private errorsHidden: boolean = true; // Toggle state for error notifications
  private pendingAttachments: MediaItem[] = []; // Files attached but not yet sent
  private isSending: boolean = false; // Guard against duplicate sends
  private positronUnsubscribe?: () => void; // Cleanup for Positron subscription
  private positronUpdateDebounce?: number; // Debounce for Positron state updates

  // === REACT-LIKE VISIBILITY STATE ===
  // Track whether this widget is the currently active/visible content
  private _isActiveContent: boolean = false; // Whether this is the currently visible chat tab
  private _isPinnedWidget: boolean = false; // Whether pinned to specific room via attribute
  private _eventUnsubscribers: Array<() => void> = []; // For cleanup on disconnect

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

    // Initialize adapter registry for per-message adapter selection
    this.adapterRegistry = new AdapterRegistry();

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
    console.log(`📨 ChatWidget: ${isActive ? 'ACTIVATED' : 'DEACTIVATED'} (${reason})`);

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
   * Called by all room-related events (ROOM_SELECTED, content:opened, content:switched).
   */
  private async handleRoomSwitch(roomIdOrUniqueId: string, roomName?: string, source?: string): Promise<void> {
    // Skip if pinned widget (handled elsewhere)
    if (this._isPinnedWidget) {
      console.log(`📨 ChatWidget: Ignoring ${source || 'room switch'} - pinned widget`);
      return;
    }

    // Skip if already on this room (compare both ID and uniqueId)
    if (roomIdOrUniqueId === this.currentRoomId) {
      console.log(`📨 ChatWidget: Already on room ${roomIdOrUniqueId}, skipping`);
      return;
    }

    console.log(`🏠 ChatWidget: Switching to room "${roomName || roomIdOrUniqueId}" (${source || 'direct'})`);

    // Use the full switchToRoom method for proper resolution
    await this.switchToRoom(roomIdOrUniqueId);
  }

  private async switchToRoom(roomIdOrName: string): Promise<void> {
    // Try to find room by uniqueId first, then by ID
    try {
      let roomId: UUID | undefined;
      let roomName: string = roomIdOrName;

      const result = await this.executeCommand<DataListParams, DataListResult<RoomEntity>>(DATA_COMMANDS.LIST, {
        collection: 'rooms',
        filter: { uniqueId: roomIdOrName },
        limit: 1
      });

      if (result.success && result.items?.[0]) {
        const room = result.items[0];
        roomId = room.id as UUID;
        roomName = room.displayName || room.name || roomIdOrName;
      } else {
        // Try as UUID directly
        roomId = roomIdOrName as UUID;
      }

      // Skip if already on this room
      if (roomId === this.currentRoomId) {
        console.log(`📨 ChatWidget: Already on room ${roomName}, skipping switch`);
        return;
      }

      console.log(`📨 ChatWidget: Switching to room "${roomName}" (${roomId})`);

      // Update state
      this.currentRoomId = roomId;
      this.currentRoomName = roomName;

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

      // Emit Positron widget state for AI awareness
      PositronWidgetState.emit({
        widgetType: 'chat',
        entityId: roomId,
        title: `Chat - ${roomName}`,
        metadata: {
          room: roomName,
          messageCount: this.totalMessageCount
        }
      });

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
      const postingClass = (message.metadata as any)?.posting ? ' posting' : '';
      messageElement.className = `message-row ${isCurrentUser ? 'right' : 'left'}${postingClass}`;
      // CRITICAL: Add entity ID to DOM for testing/debugging (test expects 'message-id')
      messageElement.setAttribute('message-id', message.id);
      messageElement.innerHTML = `
        <div class="message-bubble ${isCurrentUser ? 'current-user' : 'other-user'}">
          <div class="message-header">
            <span class="sender-name">${senderName}</span>
            <span class="message-time">${new Date(message.timestamp).toLocaleString()}</span>
          </div>
          <div class="message-content">
            ${contentHtml}
          </div>
        </div>
      `;

      // Initialize adapter content loading (e.g., image load handlers)
      if (adapter && adapter.handleContentLoading) {
        const contentDiv = messageElement.querySelector('.message-content');
        if (contentDiv) {
          adapter.handleContentLoading(contentDiv as HTMLElement).catch((err) => {
            console.error('Failed to handle content loading:', err);
          });
        }
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

      console.log(`🔍 DEBUG: currentRoomId type=${typeof this.currentRoomId}, value="${this.currentRoomId}"`);
      console.log(`🔍 CURSOR-WIDGET-DEBUG: cursor param=${cursor}, limit=${limit}`);

      // CRITICAL: The filter MUST be applied at the database level, not client-side
      // This ensures we only get messages for THIS room, with proper paging/cursors
      // Load NEWEST messages first (DESC) so recent messages appear after refresh
      // EntityScroller + CSS handle display order based on SCROLLER_PRESETS.CHAT direction
      const result = await Commands.execute<DataListParams<ChatMessageEntity>, DataListResult<ChatMessageEntity>>(DATA_COMMANDS.LIST, {
        collection: ChatMessageEntity.collection,
        filter: {
          roomId: this.currentRoomId,
          status: 'sent' // Only show successfully sent messages
        },
        orderBy: [{ field: 'timestamp', direction: 'desc' }], // Load NEWEST first
        limit: limit ?? 30, // Default page size matches SCROLLER_PRESETS.CHAT
        ...(cursor && { cursor: { field: 'timestamp', value: cursor, direction: 'before' } }) // 'before' = older than cursor for DESC queries
      });

      if (!result?.success || !result.items) {
        throw new Error(`Failed to load messages for room ${this.currentRoomId}: ${result?.error ?? 'Unknown error'}`);
      }

      // Reduce log spam
      // console.log(`🔧 CLAUDE-DEBUG: Database returned ${result.items.length} messages for room "${this.currentRoomId}", total count: ${result.count}`);

      // Store total count from database (not just loaded items)
      this.totalMessageCount = result.count ?? result.items.length;

      // Only filter out empty messages, NOT by roomId (database already did that)
      const validMessages = result.items.filter(msg => msg.content?.text?.trim());

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

      console.log(`🔧 CLAUDE-PAGINATION: Loaded ${this.loadedMessageCount}/${this.totalMessageCount} messages, hasMore=${hasMoreMessages} (got ${validMessages.length}/${limit ?? 30}), nextCursor=${nextCursor}`);

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

    // CRITICAL: Increment total count when new message is accepted via events
    if (shouldAdd) {
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

    console.log(`📨 ChatWidget: Initializing with React-like visibility tracking...`);

    // === STEP 1: Determine widget mode (pinned vs. dynamic) ===
    const roomAttr = this.getAttribute('room');
    this._isPinnedWidget = !!roomAttr;

    if (this._isPinnedWidget && roomAttr) {
      // Pinned widget (e.g., right panel) - load specific room, ignore events
      console.log(`📨 ChatWidget: PINNED to room "${roomAttr}" - will ignore content events`);
      await this.switchToRoom(roomAttr);
      this._isActiveContent = true; // Pinned widgets are always "active"
    } else {
      // Dynamic widget - use pageState as single source of truth
      await this.initializeDynamicWidget();
    }

    // === STEP 2: Load initial room data ===
    if (this.currentRoomId) {
      await this.loadRoomData(this.currentRoomId);
      this.updateHeader();
    }

    // === STEP 3: Setup event subscriptions (with cleanup tracking) ===
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

    console.log(`✅ ChatWidget: Initialized (pinned=${this._isPinnedWidget}, active=${this._isActiveContent})`);
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
      console.log(`📨 ChatWidget: Using pageState room="${this.pageState.entityId}"`);
      await this.switchToRoom(this.pageState.entityId);
      this._isActiveContent = true;
    } else {
      const entityIdAttr = this.getAttribute('entity-id') || this.getAttribute('data-entity-id');
      if (entityIdAttr) {
        console.log(`📨 ChatWidget: Using entity-id="${entityIdAttr}" from attribute (legacy)`);
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
   * Setup content-related event subscriptions (ROOM_SELECTED, content:opened, content:switched)
   * CONSOLIDATED: All three events now use the same handleRoomSwitch method
   */
  private setupContentEventSubscriptions(): void {
    // Skip all content events for pinned widgets
    if (this._isPinnedWidget) {
      console.log(`📨 ChatWidget: Skipping content event subscriptions (pinned widget)`);
      return;
    }

    // ROOM_SELECTED - from sidebar clicks
    this.subscribeWithCleanup(UI_EVENTS.ROOM_SELECTED, (data: { roomId: string; roomName: string }) => {
      this.handleRoomSwitch(data.roomId, data.roomName, 'ROOM_SELECTED');
    });

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

    console.log(`📨 ChatWidget: Content event subscriptions active (with cleanup tracking)`);
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

    // POSTED event needs extra logging for debugging
    this.subscribeWithCleanup(AI_DECISION_EVENTS.POSTED, (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.aiStatusIndicator.onPosted(data);
        this.updateHeader();
      }
    });

    console.log(`📨 ChatWidget: AI decision event subscriptions active`);
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

    console.log(`📨 ChatWidget: Learning event subscriptions active`);
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
        console.log(`🧠 ChatWidget: Profile event - ${member.displayName} status: ${member.status} → ${data.status}`);

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
    console.log(`🧹 ChatWidget: Cleaning up (${this._eventUnsubscribers.length} event subscriptions)...`);

    // Clean up ALL tracked event subscriptions (React-like cleanup pattern)
    for (const unsubscribe of this._eventUnsubscribers) {
      try {
        unsubscribe();
      } catch (e) {
        // Ignore errors during cleanup
      }
    }
    this._eventUnsubscribers = [];

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

    // Reset visibility state
    this._isActiveContent = false;

    // Call parent cleanup
    await super.disconnectedCallback();

    console.log(`✅ ChatWidget: Cleanup complete`);
  }

  /**
   * Add learning border to chat widget
   */
  private addLearningBorder(personaName: string): void {
    const container = this.shadowRoot?.querySelector('.entity-list-container') as HTMLElement;
    if (container) {
      container.classList.add('learning-active');
      container.dataset.learningPersona = personaName;
      console.log(`🧬 ChatWidget: Added learning border for ${personaName}`);
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
      console.log(`🧬 ChatWidget: Removed learning border`);
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
      console.log(`✅ ChatWidget: AI status container ready`);
    }

    // Setup error toggle handler
    this.setupErrorToggleHandler();

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
    }
  }


  /**
   * Load room data and member information
   */
  private async loadRoomData(roomId: UUID): Promise<void> {
    try {
      // Load room entity
      const roomResult = await Commands.execute<DataReadParams, DataReadResult<RoomEntity>>(DATA_COMMANDS.READ, {
        collection: RoomEntity.collection,
        id: roomId,
        backend: 'server'
      });

      if (!roomResult?.success || !roomResult.data) {
        console.error(`❌ ChatWidget: Failed to load room data for ${roomId}`);
        return;
      }

      this.currentRoom = roomResult.data;
      console.log(`✅ ChatWidget: Loaded room data with ${this.currentRoom?.members?.length ?? 0} members`);

      // Load user details for each member
      await this.loadRoomMembers();
    } catch (error) {
      console.error(`❌ ChatWidget: Error loading room data:`, error);
    }
  }

  /**
   * Load user entities for all room members
   */
  private async loadRoomMembers(): Promise<void> {
    if (!this.currentRoom) return;

    this.roomMembers.clear();

    // Load each member's user entity
    for (const member of this.currentRoom.members) {
      try {
        const userResult = await Commands.execute<DataReadParams, DataReadResult<UserEntity>>(DATA_COMMANDS.READ, {
          collection: UserEntity.collection,
          id: member.userId,
          backend: 'server'
        });

        if (userResult?.success && userResult.data) {
          this.roomMembers.set(member.userId, userResult.data);
        }
      } catch (error) {
        console.warn(`⚠️ ChatWidget: Failed to load user ${member.userId}:`, error);
      }
    }

    console.log(`✅ ChatWidget: Loaded ${this.roomMembers.size} member details`);
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
        console.log('📨 ChatWidget: No user session, using default General room');
        return;
      }

      // Query UserState to get current content item
      const listResult = await Commands.execute<DataListParams<any>, DataListResult<any>>(DATA_COMMANDS.LIST, {
        collection: 'user_states',
        filter: { userId: sessionResult.userId },
        limit: 1
      });

      if (!listResult?.success || !listResult.items?.length) {
        console.log('📨 ChatWidget: No UserState found, using default General room');
        return;
      }

      const userState = listResult.items[0];
      const currentItemId = userState.contentState?.currentItemId;
      const openItems = userState.contentState?.openItems || [];

      // Find the current content item
      const currentItem = openItems.find((item: any) => item.id === currentItemId);
      if (!currentItem) {
        console.log('📨 ChatWidget: No current content item, using default General room');
        return;
      }

      // Only use if it's a chat type with an entityId (room ID)
      if (currentItem.type === 'chat' && currentItem.entityId) {
        console.log(`📨 ChatWidget: Loading room from UserState: "${currentItem.title}" (${currentItem.entityId})`);
        this.currentRoomId = currentItem.entityId as UUID;
        this.currentRoomName = currentItem.title || 'Chat';
      } else {
        console.log(`📨 ChatWidget: Current content is "${currentItem.type}", not a chat room`);
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
   */
  private updateHeader(): void {
    // Clear any pending update
    if (this.headerUpdateTimeout) {
      clearTimeout(this.headerUpdateTimeout);
    }

    // Schedule update for next frame
    this.headerUpdateTimeout = setTimeout(() => {
      const headerElement = this.shadowRoot.querySelector('.entity-list-header');
      if (headerElement) {
        headerElement.innerHTML = this.renderHeader();
        // Reattach handlers after header update
        this.setupErrorToggleHandler();
        this.setupMemberClickHandlers();
      }
      // Update the compact status summary line
      this.updateStatusSummary();
      this.headerUpdateTimeout = undefined;
    }, 0) as unknown as number;
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
        console.log(`🔍 ChatWidget: Clicked persona ${personaId} (error=${hasError})`);

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
        console.log('🔧 ENTER-KEY-PRESSED - text:', this.messageInput?.value, 'attachments:', this.pendingAttachments.length);
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
      console.log('🔧 SEND-ALREADY-IN-PROGRESS - ignoring duplicate call');
      return;
    }

    this.isSending = true;
    console.log('🔧 SEND-MESSAGE-CALLED-' + Date.now());

    if (!this.messageInput) {
      this.isSending = false;
      return;
    }

    const text = this.messageInput.value.trim();
    console.log('🔧 MESSAGE-TEXT-LENGTH-' + text.length);
    console.log('🔧 PENDING-ATTACHMENTS-COUNT-' + this.pendingAttachments.length);

    // Must have either text or attachments
    if (!text && this.pendingAttachments.length === 0) {
      console.log('🔧 SEND-ABORTED-NO-CONTENT');
      return;
    }

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
    messageEntity.content = {
      text,
      media: this.pendingAttachments.length > 0 ? this.pendingAttachments : undefined
    };
    messageEntity.status = 'sent';
    messageEntity.priority = 'normal';
    messageEntity.timestamp = new Date();
    messageEntity.reactions = [];

    console.log('🔧 MESSAGE-ENTITY-MEDIA-COUNT-' + (messageEntity.content.media?.length ?? 0));
    if (messageEntity.content.media && messageEntity.content.media.length > 0) {
      console.log('🔧 FIRST-MEDIA-ITEM-' + JSON.stringify({
        type: messageEntity.content.media[0].type,
        filename: messageEntity.content.media[0].filename,
        size: messageEntity.content.media[0].size,
        base64Length: messageEntity.content.media[0].base64?.length ?? 0
      }));
    }

    // 🚀 OPTIMISTIC UPDATE: Clear input immediately for instant feedback
    this.messageInput.value = '';
    const attachmentCount = this.pendingAttachments.length;
    this.pendingAttachments = [];
    this.messageInput.placeholder = 'Type a message... (or drag & drop files)';

    // Reset textarea height to single row
    this.autoGrowTextarea();

    // Send in background without awaiting (non-blocking)
    // Entity events will update UI when server responds
    Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>(DATA_COMMANDS.CREATE, {
      collection: ChatMessageEntity.collection,
      data: messageEntity,
      backend: 'server'
    }).then(result => {
      console.log('🔧 COMMAND-RESULT-' + JSON.stringify(result).substring(0, 200));
      console.log(`✅ Message sent${attachmentCount > 0 ? ` with ${attachmentCount} attachment(s)` : ''}`);

      // Scroll to bottom after message added by event system
      if (this.scroller) {
        this.scroller.scrollToEnd();
      }
    }).catch(error => {
      console.error('❌ Failed to send message:', error);
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
      await Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>(DATA_COMMANDS.CREATE, {
        collection: ChatMessageEntity.collection,
        data: messageEntity,
        backend: 'server'
      });
      console.log('✅ Message sent successfully');

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

    console.log('🔧 DROP-EVENT-FIRED-' + Date.now());

    // Remove visual feedback
    const container = this.shadowRoot?.querySelector('.entity-list-container') as HTMLElement;
    if (container) {
      container.classList.remove('drag-over');
    }

    const files = e.dataTransfer?.files;
    console.log('🔧 FILES-DROPPED-COUNT-' + (files?.length ?? 0));

    if (files && files.length > 0) {
      console.log(`📎 Processing ${files.length} dropped file(s)...`);

      // Convert all files to MediaItems
      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        console.log('🔧 PROCESSING-FILE-' + file.name + '-TYPE-' + file.type + '-SIZE-' + file.size);
        try {
          const mediaItem = await this.fileToMediaItem(file);
          this.pendingAttachments.push(mediaItem);
          console.log(`✅ Added ${file.name} (${this.formatFileSize(file.size)})`);
          console.log('🔧 PENDING-ATTACHMENTS-NOW-' + this.pendingAttachments.length);
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
