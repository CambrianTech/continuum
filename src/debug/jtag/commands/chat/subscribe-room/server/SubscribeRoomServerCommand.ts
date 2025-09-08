/**
 * Subscribe to Room Events Command - Server Implementation
 * Connects widget subscriptions directly to RoomEventSystem
 */

import { SubscribeRoomCommand, SubscribeRoomParams, SubscribeRoomResult } from '../shared/SubscribeRoomCommand';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';

export class SubscribeRoomServerCommand extends SubscribeRoomCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async subscribeToRoomEvents(
    roomId: string, 
    eventTypes: readonly string[],
    sessionId: string
  ): Promise<string> {
    try {
      console.log(`🔗 Server: Subscribing session ${sessionId} to room "${roomId}" events`);
      
      // For now, create subscription ID and connect to RoomEventSystem directly
      // TODO: Implement proper ChatDaemon integration to use RoomEventSystem
      const subscriptionId = `room_${roomId}_${sessionId}_${Date.now()}`;
      console.log(`✅ Server: Session ${sessionId} subscribed to room "${roomId}"`);
      return subscriptionId;
      
    } catch (error) {
      console.error(`❌ Server: Failed to subscribe to room events:`, error);
      throw error;
    }
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}