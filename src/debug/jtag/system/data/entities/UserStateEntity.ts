/**
 * UserState Entity - Dynamic Content State Management
 *
 * Manages each user's open content tabs, current focus, and persistent UI state
 * Replaces hardcoded room IDs with dynamic, database-backed state management
 * Enables multi-device synchronization and AI agent content manipulation
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// Content state types for dynamic content management
export type ContentType = 'chat' | 'document' | 'user-profile' | 'system-config' | 'widget-debug' | 'data-explorer' | 'browser';
export type ContentPriority = 'low' | 'normal' | 'high' | 'urgent';

export interface ContentItem {
  id: UUID;
  type: ContentType;
  entityId: UUID;           // ID of the entity being displayed (roomId, userId, etc.)
  title: string;            // Display title for the tab/content
  subtitle?: string;        // Optional subtitle or status
  lastAccessedAt: Date;
  priority: ContentPriority;
  metadata?: Record<string, unknown>; // Type-specific metadata (scroll position, filters, etc.)
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
    currentWorkingDir: string;    // Current directory (default: src/debug/jtag)
    history?: string[];           // Command history (optional, for future use)
    environment?: Record<string, string>; // Environment variables (optional)
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
    this.preferences = {
      maxOpenTabs: 10,
      autoCloseAfterDays: 30,
      rememberScrollPosition: true,
      syncAcrossDevices: true
    };
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
      if (!item.id || !item.type || !item.entityId || !item.title) {
        return { success: false, error: 'UserState contentItem must have id, type, entityId, and title' };
      }

      const validTypes: ContentType[] = ['chat', 'document', 'user-profile', 'system-config', 'widget-debug', 'data-explorer', 'browser'];
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

    if (this.preferences.maxOpenTabs < 1 || this.preferences.maxOpenTabs > 50) {
      return { success: false, error: 'UserState preferences.maxOpenTabs must be between 1 and 50' };
    }

    return { success: true };
  }

  /**
   * Add a new content item to the user's open tabs
   */
  addContentItem(item: Omit<ContentItem, 'lastAccessedAt'>): void {
    const contentItem: ContentItem = {
      ...item,
      lastAccessedAt: new Date()
    };

    // Remove if already exists (move to front)
    this.contentState.openItems = this.contentState.openItems.filter(existing => existing.id !== item.id);

    // Add to front
    this.contentState.openItems.unshift(contentItem);

    // Enforce max tabs limit
    if (this.contentState.openItems.length > this.preferences.maxOpenTabs) {
      this.contentState.openItems = this.contentState.openItems.slice(0, this.preferences.maxOpenTabs);
    }

    // Set as current item
    this.contentState.currentItemId = item.id;
    this.contentState.lastUpdatedAt = new Date();
  }

  /**
   * Remove a content item from open tabs
   */
  removeContentItem(itemId: UUID): void {
    const wasCurrentItem = this.contentState.currentItemId === itemId;

    this.contentState.openItems = this.contentState.openItems.filter(item => item.id !== itemId);

    // If we removed the current item, set new current
    if (wasCurrentItem) {
      this.contentState.currentItemId = this.contentState.openItems.length > 0
        ? this.contentState.openItems[0].id
        : undefined;
    }

    this.contentState.lastUpdatedAt = new Date();
  }

  /**
   * Switch focus to a specific content item
   */
  setCurrentContent(itemId: UUID): boolean {
    const item = this.contentState.openItems.find(item => item.id === itemId);
    if (!item) {
      return false;
    }

    // Update last accessed time
    item.lastAccessedAt = new Date();

    // Move to front of array for recency ordering
    this.contentState.openItems = this.contentState.openItems.filter(i => i.id !== itemId);
    this.contentState.openItems.unshift(item);

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

    // If current item was removed, set new current
    if (this.contentState.currentItemId) {
      const currentExists = this.contentState.openItems.some(item => item.id === this.contentState.currentItemId);
      if (!currentExists) {
        this.contentState.currentItemId = this.contentState.openItems.length > 0
          ? this.contentState.openItems[0].id
          : undefined;
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