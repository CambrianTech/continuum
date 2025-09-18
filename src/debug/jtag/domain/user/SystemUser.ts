/**
 * SystemUser - Automated system messages and announcements
 *
 * Represents the system itself as a user entity for generating
 * welcome messages, instructions, and automated announcements.
 * Used for seeding chat rooms with helpful system messages.
 */

import { BaseUser, BaseUserData } from './BaseUser';
import { generateUUID, type UUID } from '../../system/core/types/CrossPlatformUUID';

/**
 * System User Data - Additional system-specific properties
 */
export interface SystemUserData extends BaseUserData {
  readonly citizenType: 'system';
  readonly systemRole: 'welcome' | 'announcements' | 'instructions' | 'general';
  readonly autoMessageTypes: readonly string[];
}

/**
 * Concrete SystemUser Implementation
 *
 * Handles system-generated messages like welcome messages,
 * room instructions, and automated announcements.
 */
export class SystemUser extends BaseUser {
  declare protected readonly data: SystemUserData;

  constructor(data: SystemUserData) {
    super(data);
  }

  get systemRole(): string {
    return this.data.systemRole;
  }

  get autoMessageTypes(): readonly string[] {
    return this.data.autoMessageTypes;
  }

  getAdapterType(): string {
    return 'system';
  }

  protected createInstance(baseData: BaseUserData): this {
    // Preserve system-specific data when creating new instances
    const systemData: SystemUserData = {
      ...baseData,
      citizenType: 'system' as const,
      systemRole: this.data.systemRole,
      autoMessageTypes: this.data.autoMessageTypes
    };
    return new SystemUser(systemData) as this;
  }

  /**
   * System User Factory Methods
   */
  static createWelcomeBot(options: {
    displayName?: string;
    sessionId?: UUID;
  } = {}): SystemUser {
    return new SystemUser({
      userId: generateUUID(),
      sessionId: options.sessionId || generateUUID(),
      displayName: options.displayName || 'Welcome System',
      citizenType: 'system',
      systemRole: 'welcome',
      capabilities: ['welcome-messages', 'room-instructions'],
      autoMessageTypes: ['welcome', 'room-intro'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true
    });
  }

  static createAnnouncementBot(options: {
    displayName?: string;
    sessionId?: UUID;
  } = {}): SystemUser {
    return new SystemUser({
      userId: generateUUID(),
      sessionId: options.sessionId || generateUUID(),
      displayName: options.displayName || 'System Announcements',
      citizenType: 'system',
      systemRole: 'announcements',
      capabilities: ['announcements', 'system-updates'],
      autoMessageTypes: ['announcement', 'update'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true
    });
  }

  static createInstructionBot(options: {
    displayName?: string;
    sessionId?: UUID;
  } = {}): SystemUser {
    return new SystemUser({
      userId: generateUUID(),
      sessionId: options.sessionId || generateUUID(),
      displayName: options.displayName || 'Help & Instructions',
      citizenType: 'system',
      systemRole: 'instructions',
      capabilities: ['help', 'instructions', 'tutorials'],
      autoMessageTypes: ['help', 'tutorial', 'instructions'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true
    });
  }

  /**
   * System Message Generation
   */
  generateWelcomeMessage(roomName: string): string {
    return `🎉 Welcome to the **${roomName}** room! This is where you can chat and collaborate with other users. Feel free to introduce yourself or ask questions.`;
  }

  generateInstructionMessage(): string {
    return `💡 **Getting Started:**
- Type a message in the input box below to chat
- Use @username to mention someone
- Click on usernames to view profiles
- Use the room list to switch between different topics`;
  }

  generateRoomIntroMessage(roomName: string, purpose: string): string {
    return `📌 **${roomName} Room Information**

${purpose}

This is a great place to discuss topics related to this room's focus. Please keep conversations respectful and on-topic.`;
  }
}