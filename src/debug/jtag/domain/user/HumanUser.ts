/**
 * HumanUser - Human citizen implementation
 *
 * Represents actual human users with interactive capabilities.
 * Provides human-specific adapter and interaction patterns.
 */

import { BaseUser, type BaseUserData, type UserCitizenType } from './BaseUser';

/**
 * Human-specific data extending base user
 */
export interface HumanUserData extends BaseUserData {
  readonly citizenType: 'human';
}

/**
 * Human User - Interactive human citizen
 *
 * Handles direct user interactions, authentication, and human-centric features.
 * Uses human-interactive adapter for real-time UI updates and input handling.
 */
export class HumanUser extends BaseUser {
  declare protected readonly data: HumanUserData;

  constructor(data: HumanUserData) {
    super(data);
  }

  /**
   * Human-specific implementation
   */
  getAdapterType(): string {
    return 'human-interactive';
  }


  /**
   * Factory methods
   */
  static fromData(data: HumanUserData): HumanUser {
    return new HumanUser(data);
  }

  static createNew(
    displayName: string,
    sessionId: string
  ): HumanUser {
    const data: HumanUserData = {
      userId: crypto.randomUUID(),
      sessionId,
      displayName,
      citizenType: 'human',
      capabilities: [],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true
    };
    return new HumanUser(data);
  }

  /**
   * Immutable update implementation
   */
  protected createInstance(data: BaseUserData): this {
    return new HumanUser(data as HumanUserData) as this;
  }
}