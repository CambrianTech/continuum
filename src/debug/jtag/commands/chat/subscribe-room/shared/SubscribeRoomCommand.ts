/**
 * Subscribe to Room Events Command - Connects widgets to RoomEventSystem
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

export interface SubscribeRoomParams extends CommandParams {
  readonly roomId: string;
  readonly eventTypes: readonly string[];
}

export interface SubscribeRoomResult extends CommandResult {
  readonly success: boolean;
  readonly subscriptionId?: string;
  readonly roomId: string;
  readonly error?: string;
}

export abstract class SubscribeRoomCommand extends CommandBase<SubscribeRoomParams, SubscribeRoomResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: any) {
    super('chat-subscribe-room', context, subpath, commander);
  }

  /**
   * Execute room subscription - connects widget to RoomEventSystem
   */
  async execute(params: SubscribeRoomParams): Promise<SubscribeRoomResult> {
    const startTime = Date.now();
    
    console.log(`🔗 ${this.getEnvironmentLabel()}: Subscribing to room "${params.roomId}" events`);

    try {
      // Validate parameters
      if (!params.roomId || typeof params.roomId !== 'string') {
        return this.createError(params, 'roomId is required and must be a string');
      }

      if (!params.eventTypes || !Array.isArray(params.eventTypes) || params.eventTypes.length === 0) {
        return this.createError(params, 'eventTypes array is required');
      }

      // Call the environment-specific subscription logic
      const subscriptionId = await this.subscribeToRoomEvents(params.roomId, params.eventTypes, params.sessionId);
      
      const duration = Date.now() - startTime;
      console.log(`✅ ${this.getEnvironmentLabel()}: Subscribed to room "${params.roomId}" in ${duration}ms`);

      return this.createSuccess(params, subscriptionId);

    } catch (error) {
      console.error(`❌ ${this.getEnvironmentLabel()}: Failed to subscribe to room:`, error);
      
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      return this.createError(params, errorMessage);
    }
  }

  /**
   * Subscribe to room events - implemented per environment
   */
  protected abstract subscribeToRoomEvents(
    roomId: string, 
    eventTypes: readonly string[],
    sessionId: string
  ): Promise<string>;

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;

  /**
   * Type-safe success result creation
   */
  protected createSuccess(
    params: SubscribeRoomParams,
    subscriptionId: string
  ): SubscribeRoomResult {
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      subscriptionId,
      roomId: params.roomId
    };
  }

  /**
   * Type-safe error result creation
   */
  protected createError(params: SubscribeRoomParams, error: string): SubscribeRoomResult {
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: false,
      roomId: params.roomId,
      error
    };
  }
}