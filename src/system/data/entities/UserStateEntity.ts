/**
 * UserState Entity - Dynamic Content State Management
 *
 * Manages each user's open content tabs, current focus, and persistent UI state
 * Replaces hardcoded room IDs with dynamic, database-backed state management
 * Enables multi-device synchronization and AI agent content manipulation
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// Content state types for dynamic content management
export type ContentType = 'chat' | 'document' | 'user-profile' | 'profile' | 'system-config' | 'widget-debug' | 'data-explorer' | 'browser' | 'settings' | 'help' | 'theme' | 'persona' | 'diagnostics' | 'diagnostics-log' | 'canvas' | 'live';
export type ContentPriority = 'low' | 'normal' | 'high' | 'urgent';

export interface ContentItem {
  id: UUID;
  type: ContentType;
  entityId?: UUID;          // ID of the entity being displayed (roomId, userId, etc.) - optional for singleton content like settings
  uniqueId?: string;        // Human-readable identifier for URLs (e.g., "general" instead of UUID)
  title: string;            // Display title for the tab/content
  subtitle?: string;        // Optional subtitle or status
  lastAccessedAt: Date;
  priority: ContentPriority;
  metadata?: Record<string, unknown>; // Type-specific metadata (scroll position, filters, etc.)
}

/**
 * Check if two ContentItems represent the same logical content.
 * Matches by type AND (entityId OR uniqueId OR both undefined for singletons).
 *
 * IMPORTANT: Same uniqueId = same logical entity, even if entityIds differ.
 * This handles inconsistent call sites where entityId might be UUID or uniqueId string.
 *
 * @param a First content item (or partial with type/entityId/uniqueId)
 * @param b Second content item (or partial with type/entityId/uniqueId)
 * @returns true if they represent the same content
 */
export function contentItemsMatch(
  a: Pick<ContentItem, 'type'> & Partial<Pick<ContentItem, 'entityId' | 'uniqueId'>>,
  b: Pick<ContentItem, 'type'> & Partial<Pick<ContentItem, 'entityId' | 'uniqueId'>>
): boolean {
  // Different types = different content
  if (a.type !== b.type) return false;

  // Singleton content (no entityId or uniqueId) - match by type only
  // e.g., settings, help, theme tabs that have no associated entity
  const aIssingleton = !a.entityId && !a.uniqueId;
  const bIsSingleton = !b.entityId && !b.uniqueId;
  if (aIssingleton && bIsSingleton) return true;

  // Same entityId = same content
  if (a.entityId && b.entityId && a.entityId === b.entityId) return true;

  // Same uniqueId = same content (handles UUID vs uniqueId string mismatch)
  if (a.uniqueId && b.uniqueId && a.uniqueId === b.uniqueId) return true;

  // Cross-match: entityId matches uniqueId (handles case where one side used uniqueId as entityId)
  if (a.entityId && b.uniqueId && a.entityId === b.uniqueId) return true;
  if (a.uniqueId && b.entityId && a.uniqueId === b.entityId) return true;

  return false;
}

export interface ContentState {
  openItems: ContentItem[];  // Array of open content tabs
  currentItemId?: UUID;      // Currently focused content item
  lastUpdatedAt: Date;
}

import {
  TextField,
  JsonField,
  ForeignKeyField
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { USER_PREFERENCES_DEFAULTS } from '../../user/config/UserCapabilitiesDefaults';

/**
 * UserState Entity - Per-user dynamic content state
 *
 * Manages what content each user has open across all their sessions/devices
 * Enables seamless multi-device experience and AI content manipulation
 */
export class UserStateEntity extends BaseEntity {
  // Single source of truth for collection name
  static readonly collection = 'user_states';

  @ForeignKeyField({ references: 'User.id' })
  userId: UUID;

  @TextField({ index: true })
  deviceId: string; // Optional device identifier for multi-device sync

  @JsonField()
  contentState: ContentState;

  // User preferences for content management
  @JsonField()
  preferences: {
    maxOpenTabs: number;
    autoCloseAfterDays: number;
    rememberScrollPosition: boolean;
    syncAcrossDevices: boolean;
  };

  // Room/subscription read state - tracks last processed message per room
  // Used by both human users (unread messages) and AI personas (message processing pointer)
  @JsonField()
  roomReadState: {
    [roomId: string]: {
      lastReadMessageTimestamp: string; // ISO timestamp of last read/processed message
      lastReadMessageId?: UUID;          // Optional message ID for exact tracking
    };
  };

  // AI learning state - tracks whether PersonaUser is currently training
  // Used by UI widgets to show learning indicators (similar to AI thinking status)
  @JsonField()
  learningState?: {
    isLearning: boolean;
    domain?: string;              // Domain being trained (e.g., 'conversational', 'code')
    provider?: string;            // Training provider (e.g., 'unsloth', 'deepseek')
    startedAt?: number;          // Timestamp when training started
    exampleCount?: number;        // Number of training examples
    estimatedCompletion?: number; // Estimated completion timestamp
  };

  // AI dormancy state - controls PersonaUser engagement levels
  // Used to implement self-regulated dormancy and auto-dormancy rules
  // NOTE: No 'sleep' mode - @mentions ALWAYS work as failsafe
  @JsonField()
  dormancyState?: {
    level: 'active' | 'mention-only' | 'human-only';
    reason?: string;              // Optional: Why dormancy was activated
    setAt?: string;               // ISO timestamp when dormancy was activated
    until?: string;               // Optional: Auto-wake timestamp (ISO 8601)
    setBy?: 'self' | 'autopilot' | 'human'; // Who initiated dormancy
  };

  // Shell state - tracks current working directory for shell commands
  // Used by code/find, tree, and other filesystem operations
  // Enables cd, pwd, and path-relative operations per-user
  @JsonField()
  shellState?: {
    currentWorkingDir: string;    // Current directory (default: src)
    history?: string[];           // Command history (optional, for future use)
    environment?: Record<string, string>; // Environment variables (optional)
  };

  // Voice/Call state - tracks user's mic and speaker settings in live calls
  // Persisted across sessions and synced to UI components
  @JsonField()
  callState?: {
    micEnabled: boolean;           // User's microphone enabled
    speakerEnabled: boolean;       // User's audio output enabled
    speakerVolume: number;         // 0.0 to 1.0
    cameraEnabled: boolean;        // User's camera enabled
    screenShareEnabled: boolean;   // User is screen sharing
    currentCallId?: string;        // Active call ID (if in a call)
  };

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super(); // Initialize BaseEntity fields

    // Default values
    this.userId = '' as UUID;
    this.deviceId = 'default';
    this.contentState = {
      openItems: [],
      lastUpdatedAt: new Date()
    };
    // Use agent defaults as reasonable fallback (overridden by user type on creation)
    this.preferences = { ...USER_PREFERENCES_DEFAULTS.agent };
    this.roomReadState = {};
    this.learningState = {
      isLearning: false
    };
    this.dormancyState = {
      level: 'active' // Default: fully active
    };
    this.shellState = {
      currentWorkingDir: typeof process !== 'undefined' ? process.cwd() : '/'
    };
    this.callState = {
      micEnabled: true,
      speakerEnabled: true,
      speakerVolume: 1.0,
      cameraEnabled: false,
      screenShareEnabled: false
    };
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return UserStateEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate user state data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields validation
    if (!this.userId?.trim()) {
      return { success: false, error: 'UserState userId is required' };
    }

    if (!this.deviceId?.trim()) {
      return { success: false, error: 'UserState deviceId is required' };
    }

    // Validate contentState structure
    if (!this.contentState) {
      return { success: false, error: 'UserState contentState is required' };
    }

    if (!Array.isArray(this.contentState.openItems)) {
      return { success: false, error: 'UserState contentState.openItems must be an array' };
    }

    // Validate each content item
    for (const item of this.contentState.openItems) {
      if (!item.id || !item.type || !item.title) {
        return { success: false, error: 'UserState contentItem must have id, type, and title' };
      }

      const validTypes: ContentType[] = ['chat', 'document', 'user-profile', 'profile', 'system-config', 'widget-debug', 'data-explorer', 'browser', 'settings', 'help', 'theme', 'persona', 'diagnostics', 'diagnostics-log', 'canvas', 'live'];
      if (!validTypes.includes(item.type)) {
        return { success: false, error: `UserState contentItem type must be one of: ${validTypes.join(', ')}` };
      }
    }

    // Validate current item exists if specified
    if (this.contentState.currentItemId) {
      const currentExists = this.contentState.openItems.some(item => item.id === this.contentState.currentItemId);
      if (!currentExists) {
        return { success: false, error: 'UserState currentItemId must reference an existing open item' };
      }
    }

    // Validate preferences
    if (!this.preferences || typeof this.preferences.maxOpenTabs !== 'number') {
      return { success: false, error: 'UserState preferences.maxOpenTabs must be a number' };
    }

    if (this.preferences.maxOpenTabs < 1 || this.preferences.maxOpenTabs > 200) {
      return { success: false, error: 'UserState preferences.maxOpenTabs must be between 1 and 200' };
    }

    return { success: true };
  }

  /**
   * Add a new content item to the user's open tabs
   * Deduplicates using contentItemsMatch() - clicking same room just switches focus
   * Tabs maintain insertion order - like VSCode, not MRU
   */
  addContentItem(item: Omit<ContentItem, 'lastAccessedAt'>): void {
    // Check if this entity is already open using centralized matching
    const existingItem = this.contentState.openItems.find(
      existing => contentItemsMatch(existing, item)
    );

    if (existingItem) {
      // Already open - just switch to it (NO reordering, just update timestamp and focus)
      existingItem.lastAccessedAt = new Date();

      // Update missing fields that may have been added later (e.g., uniqueId for URLs)
      if (item.uniqueId && !existingItem.uniqueId) {
        existingItem.uniqueId = item.uniqueId;
      }
      // Fix title if it was incorrectly set to UUID
      if (item.title && existingItem.title === existingItem.entityId) {
        existingItem.title = item.title;
      }

      this.contentState.currentItemId = existingItem.id;
      this.contentState.lastUpdatedAt = new Date();
      return;
    }

    // New content - create item with timestamp
    const contentItem: ContentItem = {
      ...item,
      lastAccessedAt: new Date()
    };

    // Add to END (append) - maintains insertion order
    this.contentState.openItems.push(contentItem);

    // Enforce max tabs limit - remove oldest (from start) if over limit
    while (this.contentState.openItems.length > this.preferences.maxOpenTabs) {
      this.contentState.openItems.shift();
    }

    // Set as current item
    this.contentState.currentItemId = item.id;
    this.contentState.lastUpdatedAt = new Date();
  }

  /**
   * Remove a content item from open tabs
   * If closing the current tab, switches to most recently accessed tab (browser-style history)
   */
  removeContentItem(itemId: UUID): void {
    const wasCurrentItem = this.contentState.currentItemId === itemId;

    this.contentState.openItems = this.contentState.openItems.filter(item => item.id !== itemId);

    // If we removed the current item, switch to most recently accessed tab
    if (wasCurrentItem && this.contentState.openItems.length > 0) {
      // Sort by lastAccessedAt descending to find most recently viewed
      const mostRecent = [...this.contentState.openItems].sort((a, b) => {
        const aTime = new Date(a.lastAccessedAt).getTime();
        const bTime = new Date(b.lastAccessedAt).getTime();
        return bTime - aTime;
      })[0];
      this.contentState.currentItemId = mostRecent.id;
    } else if (this.contentState.openItems.length === 0) {
      this.contentState.currentItemId = undefined;
    }

    this.contentState.lastUpdatedAt = new Date();
  }

  /**
   * Switch focus to a specific content item
   * Maintains insertion order - like VSCode, not MRU
   */
  setCurrentContent(itemId: UUID): boolean {
    const item = this.contentState.openItems.find(item => item.id === itemId);
    if (!item) {
      return false;
    }

    // Update last accessed time (NO reordering - tabs stay in place)
    item.lastAccessedAt = new Date();

    // Set as current
    this.contentState.currentItemId = itemId;
    this.contentState.lastUpdatedAt = new Date();

    return true;
  }

  /**
   * Get current content item
   */
  getCurrentContentItem(): ContentItem | undefined {
    if (!this.contentState.currentItemId) {
      return undefined;
    }

    return this.contentState.openItems.find(item => item.id === this.contentState.currentItemId);
  }

  /**
   * Clean up old content items based on preferences
   */
  cleanupOldContent(): void {
    if (this.preferences.autoCloseAfterDays <= 0) {
      return; // No auto-cleanup
    }

    const cutoffDate = new Date();
    cutoffDate.setDate(cutoffDate.getDate() - this.preferences.autoCloseAfterDays);

    const initialLength = this.contentState.openItems.length;
    this.contentState.openItems = this.contentState.openItems.filter(item =>
      item.lastAccessedAt > cutoffDate || item.priority === 'urgent'
    );

    // If current item was removed, switch to most recently accessed
    if (this.contentState.currentItemId) {
      const currentExists = this.contentState.openItems.some(item => item.id === this.contentState.currentItemId);
      if (!currentExists && this.contentState.openItems.length > 0) {
        const mostRecent = [...this.contentState.openItems].sort((a, b) => {
          const aTime = new Date(a.lastAccessedAt).getTime();
          const bTime = new Date(b.lastAccessedAt).getTime();
          return bTime - aTime;
        })[0];
        this.contentState.currentItemId = mostRecent.id;
      } else if (this.contentState.openItems.length === 0) {
        this.contentState.currentItemId = undefined;
      }
    }

    // Update timestamp if we cleaned anything
    if (this.contentState.openItems.length !== initialLength) {
      this.contentState.lastUpdatedAt = new Date();
    }
  }

  /**
   * Update last read message timestamp for a room
   * Used by both human users (unread messages) and AI personas (message processing pointer)
   */
  updateRoomReadState(roomId: UUID, timestamp: Date, messageId?: UUID): void {
    if (!this.roomReadState) {
      this.roomReadState = {};
    }

    this.roomReadState[roomId] = {
      lastReadMessageTimestamp: timestamp.toISOString(),
      lastReadMessageId: messageId
    };
  }

  /**
   * Get last read message timestamp for a room
   * Returns undefined if never read
   */
  getLastReadTimestamp(roomId: UUID): Date | undefined {
    const readState = this.roomReadState?.[roomId];
    if (!readState?.lastReadMessageTimestamp) {
      return undefined;
    }

    return new Date(readState.lastReadMessageTimestamp);
  }

  /**
   * Check if a message timestamp is newer than last read
   * Used to determine unread messages
   */
  isMessageUnread(roomId: UUID, messageTimestamp: Date): boolean {
    const lastRead = this.getLastReadTimestamp(roomId);
    if (!lastRead) {
      return true; // Never read any messages in this room
    }

    return messageTimestamp > lastRead;
  }
}