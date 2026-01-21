/**
 * Call Entity - Tracks live audio/video collaboration sessions
 *
 * Like Slack huddles, Discord voice channels, Zoom calls.
 * Each room/activity can have an associated call.
 * Participants join/leave, toggle mic/camera/screen share.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// Call status
export type CallStatus = 'active' | 'ended';

// Participant state in the call
export interface CallParticipant {
  userId: UUID;
  displayName: string;
  avatar?: string;
  joinedAt: Date;
  leftAt?: Date;
  micEnabled: boolean;
  cameraEnabled: boolean;
  screenShareEnabled: boolean;
  // Server-side audio handle for mixing
  audioStreamHandle?: string;
  // Server-side video handle for relaying
  videoStreamHandle?: string;
}

import {
  TextField,
  DateField,
  EnumField,
  JsonField,
  ForeignKeyField,
  NumberField
} from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';

/**
 * Call Entity - Tracks participants in a live collaboration call
 *
 * One call per room at a time. When all participants leave, call ends.
 * Historical calls kept for analytics.
 */
export class CallEntity extends BaseEntity {
  // Single source of truth for collection name
  static readonly collection = 'calls';

  @ForeignKeyField({ references: 'rooms.id', index: true })
  roomId: UUID;

  @EnumField({ index: true })
  status: CallStatus;

  @DateField({ nullable: true })
  endedAt?: Date;

  // Active participants (updated on join/leave/toggle)
  @JsonField()
  participants: CallParticipant[];

  // Peak concurrent participants (for analytics)
  @NumberField()
  peakParticipants: number;

  // Total unique participants who joined (for analytics)
  @NumberField()
  totalParticipants: number;

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super(); // Initialize BaseEntity fields

    // Default values
    this.roomId = '' as UUID;
    this.status = 'active';
    this.participants = [];
    this.peakParticipants = 0;
    this.totalParticipants = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return CallEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate call data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.roomId?.trim()) {
      return { success: false, error: 'Call roomId is required' };
    }

    const validStatuses: CallStatus[] = ['active', 'ended'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `Call status must be one of: ${validStatuses.join(', ')}` };
    }

    if (!Array.isArray(this.participants)) {
      return { success: false, error: 'Call participants must be an array' };
    }

    return { success: true };
  }

  /**
   * Add a participant to the call
   */
  addParticipant(userId: UUID, displayName: string, avatar?: string): CallParticipant {
    // Check if already in call
    const existing = this.participants.find(p => p.userId === userId && !p.leftAt);
    if (existing) {
      return existing;
    }

    const participant: CallParticipant = {
      userId,
      displayName,
      avatar,
      joinedAt: new Date(),
      micEnabled: false,  // Start muted by default
      cameraEnabled: false,
      screenShareEnabled: false
    };

    this.participants.push(participant);
    this.totalParticipants++;

    const activeCount = this.getActiveParticipants().length;
    if (activeCount > this.peakParticipants) {
      this.peakParticipants = activeCount;
    }

    return participant;
  }

  /**
   * Remove a participant from the call
   */
  removeParticipant(userId: UUID): boolean {
    const participant = this.participants.find(p => p.userId === userId && !p.leftAt);
    if (!participant) {
      return false;
    }

    participant.leftAt = new Date();

    // End call if no active participants
    if (this.getActiveParticipants().length === 0) {
      this.status = 'ended';
      this.endedAt = new Date();
    }

    return true;
  }

  /**
   * Get currently active participants (joined but not left)
   */
  getActiveParticipants(): CallParticipant[] {
    return this.participants.filter(p => !p.leftAt);
  }

  /**
   * Update participant media state
   */
  updateParticipantMedia(
    userId: UUID,
    updates: Partial<Pick<CallParticipant, 'micEnabled' | 'cameraEnabled' | 'screenShareEnabled' | 'audioStreamHandle' | 'videoStreamHandle'>>
  ): boolean {
    const participant = this.participants.find(p => p.userId === userId && !p.leftAt);
    if (!participant) {
      return false;
    }

    Object.assign(participant, updates);
    return true;
  }

  /**
   * Check if a user is currently in the call
   */
  isParticipant(userId: UUID): boolean {
    return this.participants.some(p => p.userId === userId && !p.leftAt);
  }

  /**
   * Get participant by userId
   */
  getParticipant(userId: UUID): CallParticipant | undefined {
    return this.participants.find(p => p.userId === userId && !p.leftAt);
  }
}
