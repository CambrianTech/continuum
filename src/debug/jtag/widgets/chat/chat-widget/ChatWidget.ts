/**
 * Chat Widget - Now extends EntityScrollerWidget for automatic CRUD management
 * Eliminates ~300 lines of manual EntityScroller and CRUD event handling
 */

import { EntityScrollerWidget } from '../../shared/EntityScrollerWidget';
import { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { DataReadParams, DataReadResult } from '../../../commands/data/read/shared/DataReadTypes';
import { Commands } from '../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
import { Events } from '../../../system/core/shared/Events';
import { UI_EVENTS } from '../../../system/core/shared/EventConstants';
import { SCROLLER_PRESETS, type RenderFn, type LoadFn, type ScrollerConfig } from '../../shared/EntityScroller';
import { DEFAULT_ROOMS, DEFAULT_USERS } from '../../../system/data/domains/DefaultEntities';
import { AdapterRegistry } from '../adapters/AdapterRegistry';
import { AbstractMessageAdapter } from '../adapters/AbstractMessageAdapter';
import { MessageInputEnhancer } from '../message-input/MessageInputEnhancer';
import { AIStatusIndicator } from './AIStatusIndicator';
import { AI_DECISION_EVENTS } from '../../../system/events/shared/AIDecisionEvents';

export class ChatWidget extends EntityScrollerWidget<ChatMessageEntity> {
  private messageInput?: HTMLInputElement;
  private currentRoomId: UUID | null = DEFAULT_ROOMS.GENERAL as UUID; // Default to General room
  private currentRoomName: string = 'General';
  private currentRoom: RoomEntity | null = null;
  private roomMembers: Map<UUID, UserEntity> = new Map(); // Map of userId -> UserEntity
  private totalMessageCount: number = 0; // Total messages in database (not just loaded)
  private loadedMessageCount: number = 0; // Number of messages actually loaded so far
  private adapterRegistry: AdapterRegistry; // Selects adapters per message based on content
  private inputEnhancer?: MessageInputEnhancer; // Markdown shortcuts for message input
  private aiStatusIndicator: AIStatusIndicator; // Manages AI thinking/responding status indicators
  private aiStatusContainer?: HTMLElement; // Container for AI status indicators

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
      messageElement.className = `message-row ${isCurrentUser ? 'right' : 'left'}`;
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
    return SCROLLER_PRESETS.CHAT; // Auto-scroll to bottom for new messages
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

    console.log(`📨 ChatWidget: Enabled ChatMessage event subscriptions for real-time updates`);

    // Load initial room data (General room by default)
    if (this.currentRoomId) {
      await this.loadRoomData(this.currentRoomId);
      this.updateHeader();
    }

    // Listen for room selection events
    Events.subscribe(UI_EVENTS.ROOM_SELECTED, async (eventData: { roomId: string; roomName: string }) => {
      console.log(`🏠 ChatWidget: Room selected "${eventData.roomName}" (${eventData.roomId})`);
      this.currentRoomId = eventData.roomId as UUID;
      this.currentRoomName = eventData.roomName;

      // Reset counters for new room
      this.totalMessageCount = 0;
      this.loadedMessageCount = 0;

      // Clear AI status indicators for previous room
      this.aiStatusIndicator.clearAll();

      // Update header immediately to show new room name (count will be 0 initially)
      this.updateHeader();

      // Load room data and members, then refresh messages asynchronously
      Promise.all([
        this.loadRoomData(this.currentRoomId),
        this.scroller?.refresh()
      ]).then(() => {
        // Update header again with correct message count after refresh completes
        this.updateHeader();
      });
    });

    // Subscribe to AI decision events for real-time thinking/responding indicators
    // Note: We subscribe globally and filter by room ID in handlers
    console.log(`🤖 ChatWidget: Subscribing to AI decision events`);

    Events.subscribe(AI_DECISION_EVENTS.EVALUATING, (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.aiStatusIndicator.onEvaluating(data);
      }
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_RESPOND, (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.aiStatusIndicator.onDecidedRespond(data);
      }
    });

    Events.subscribe(AI_DECISION_EVENTS.DECIDED_SILENT, (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.aiStatusIndicator.onDecidedSilent(data);
      }
    });

    Events.subscribe(AI_DECISION_EVENTS.GENERATING, (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.aiStatusIndicator.onGenerating(data);
      }
    });

    Events.subscribe(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.aiStatusIndicator.onCheckingRedundancy(data);
      }
    });

    Events.subscribe(AI_DECISION_EVENTS.POSTED, (data: any) => {
      console.log(`🔧 ChatWidget: Received POSTED event for ${data.personaName}, roomId: ${data.roomId}, currentRoomId: ${this.currentRoomId}`);
      if (data.roomId === this.currentRoomId) {
        console.log(`✅ ChatWidget: Room matches, calling aiStatusIndicator.onPosted`);
        this.aiStatusIndicator.onPosted(data);
      } else {
        console.log(`⚠️ ChatWidget: Room doesn't match, ignoring POSTED event`);
      }
    });

    Events.subscribe(AI_DECISION_EVENTS.ERROR, (data: any) => {
      if (data.roomId === this.currentRoomId) {
        this.aiStatusIndicator.onError(data);
      }
    });

    console.log(`✅ ChatWidget: AI decision event subscriptions active`);

    // EntityScrollerWidget automatically handles ChatMessage events via createEntityCrudHandler
    // No manual subscription needed - filtering happens in shouldAddEntity()

    console.log(`✅ ChatWidget: Initialized with room selection, AI status indicators, and automatic CRUD events`);
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
        <div class="ai-status-container" id="aiStatusContainer"></div>

        <div class="entity-list-body messages-container">
          <!-- EntityScroller will populate this container -->
        </div>

        ${this.renderFooter()}
      </div>
    `;
  }

  // Custom footer with message input - the ONLY unique part
  protected renderFooter(): string {
    return `
      <div class="input-container">
        <input type="text" class="message-input" id="messageInput" placeholder="Type a message...">
        <button class="send-button" id="sendButton">Send</button>
      </div>
    `;
  }

  // Override to setup message input after EntityScroller initialization
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
      console.log(`✅ ChatWidget: AI status container ready`);
    }

    // Cache input element after DOM is rendered
    this.messageInput = this.shadowRoot?.getElementById('messageInput') as HTMLInputElement;
    this.setupMessageInputHandlers();

    // Enable markdown shortcuts for message input (Cmd+B for bold, etc.)
    if (this.messageInput) {
      this.inputEnhancer = new MessageInputEnhancer(this.messageInput, {
        enableShortcuts: true,
        enableAutoFormat: true, // Auto-complete ``` to code blocks
        enablePreview: false // Preview can be added later if needed
      });
    }
  }

  // Setup input event listeners - the ONLY unique ChatWidget functionality
  private setupMessageInputHandlers(): void {
    if (!this.messageInput) return;

    const keydownHandler = (e: KeyboardEvent): void => {
      if (e.key === 'Enter') {
        e.preventDefault();
        this.sendMessage();
      }
    };

    const sendButton = this.shadowRoot?.getElementById('sendButton');
    const clickHandler = (): void => { this.sendMessage(); };

    this.messageInput.addEventListener('keydown', keydownHandler);
    sendButton?.addEventListener('click', clickHandler);
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
   * Override renderHeader to display room members
   */
  protected override renderHeader(): string {
    // Get member display (avatars/names)
    const memberDisplay = this.renderMemberList();

    return `
      <div class="entity-list-header">
        <div class="header-top">
          <span class="header-title">${this.currentRoomName}</span>
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

        return `
          <div class="member-chip" title="${displayName} (${role})">
            ${roleIcon}
            <span class="member-name">${displayName}</span>
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
   */
  private updateHeader(): void {
    const headerElement = this.shadowRoot.querySelector('.entity-list-header');
    if (headerElement) {
      headerElement.innerHTML = this.renderHeader();
    }
  }

  // Send message - the only business logic ChatWidget needs
  private async sendMessage(): Promise<void> {
    const content = this.messageInput?.value.trim();
    if (!content) return;

    // Can't send message without a room selected
    if (!this.currentRoomId) {
      console.warn('Cannot send message: no room selected');
      return;
    }

    this.messageInput!.value = ''; // Clear input

    // Create message entity
    const messageEntity = new ChatMessageEntity();
    messageEntity.roomId = this.currentRoomId; // Use centralized roomId
    messageEntity.senderId = DEFAULT_USERS.HUMAN as UUID;
    messageEntity.senderName = 'Joel';
    messageEntity.senderType = 'human'; // Denormalized user type (human messages from UI)
    messageEntity.content = { text: content, attachments: [] };
    messageEntity.status = 'sent'; // Message is sent when saved to DB
    messageEntity.priority = 'normal';
    messageEntity.timestamp = new Date();
    messageEntity.reactions = [];

    try {
      await Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>(DATA_COMMANDS.CREATE, {
        collection: ChatMessageEntity.collection,
        data: messageEntity,
        backend: 'server'
      });
      console.log('✅ Message sent successfully');

      // Scroll to bottom after sending OWN message (not for other users' messages)
      // This ensures user sees their sent message immediately
      if (this.scroller) {
        this.scroller.scrollToEnd();
      }
    } catch (error) {
      console.error('❌ Failed to send message:', error);
      this.messageInput!.value = content; // Restore on error
    }
  }

}
