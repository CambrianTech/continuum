/**
 * Subscribe to Room Events Command - Server Implementation
 * Connects widget subscriptions directly to RoomEventSystem
 */

import { SubscribeRoomCommand } from '../shared/SubscribeRoomCommand';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { CHAT_EVENTS } from '../../../../widgets/chat/shared/ChatEventConstants';

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

      const subscriptionId = `room_${roomId}_${sessionId}_${Date.now()}`;

      // Register room-specific subscriptions using existing router architecture
      for (const eventType of eventTypes) {
        // Validate event type against CHAT_EVENTS constants
        const validEventTypes = Object.values(CHAT_EVENTS) as readonly string[];
        if (!validEventTypes.includes(eventType)) {
          throw new Error(`Invalid event type: ${eventType}. Must be from CHAT_EVENTS constants.`);
        }

        console.log(`📡 Server: Registering subscription for ${eventType} in room ${roomId}`);

        // Store subscription for event filtering when events are emitted
        this.registerRoomSubscription(subscriptionId, roomId, eventType, sessionId);
      }

      console.log(`✅ Server: Session ${sessionId} subscribed to room "${roomId}" with ${eventTypes.length} event types`);
      return subscriptionId;

    } catch (error) {
      console.error(`❌ Server: Failed to subscribe to room events:`, error);
      throw error;
    }
  }

  /**
   * Register room subscription for event routing
   * Uses singleton pattern for cross-command subscription state
   */
  private registerRoomSubscription(
    subscriptionId: string,
    roomId: string,
    eventType: string,
    sessionId: string
  ): void {
    const subscriptionKey = `${eventType}:${roomId}`;

    // Use static storage to persist across command instances
    SubscribeRoomServerCommand.roomSubscriptions ??= new Map();

    if (!SubscribeRoomServerCommand.roomSubscriptions.has(subscriptionKey)) {
      SubscribeRoomServerCommand.roomSubscriptions.set(subscriptionKey, new Set());
    }

    SubscribeRoomServerCommand.roomSubscriptions.get(subscriptionKey)!.add(sessionId);

    console.log(`📝 Server: Registered subscription ${subscriptionId} for ${subscriptionKey} -> session ${sessionId}`);
    console.log(`📊 Server: Total subscriptions for ${subscriptionKey}: ${SubscribeRoomServerCommand.roomSubscriptions.get(subscriptionKey)!.size}`);
  }

  /**
   * Static storage for room subscriptions (singleton pattern)
   * This allows other commands (like chat/send-message) to access subscriptions
   */
  private static roomSubscriptions?: Map<string, Set<string>>;

  /**
   * Public method to get room subscriptions for event routing
   * Used by chat/send-message to determine which sessions should receive events
   */
  public static getRoomSubscriptions(eventType: string, roomId: string): Set<string> | undefined {
    const subscriptionKey = `${eventType}:${roomId}`;
    return SubscribeRoomServerCommand.roomSubscriptions?.get(subscriptionKey);
  }

  protected getEnvironmentLabel(): string {
    return 'SERVER';
  }
}