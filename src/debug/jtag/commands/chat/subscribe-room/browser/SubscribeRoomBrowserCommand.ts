/**
 * Subscribe to Room Events Command - Browser Implementation  
 * Delegates to server via remoteExecute
 */

import { SubscribeRoomCommand, SubscribeRoomParams, SubscribeRoomResult } from '../shared/SubscribeRoomCommand';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';

export class SubscribeRoomBrowserCommand extends SubscribeRoomCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async subscribeToRoomEvents(
    roomId: string, 
    eventTypes: readonly string[],
    sessionId: string
  ): Promise<string> {
    try {
      console.log(`🔗 Browser: Requesting room subscription via server for room "${roomId}"`);
      
      // Browser delegates to server for room event subscription  
      const serverParams: SubscribeRoomParams = {
        context: this.context,
        sessionId: this.context.uuid,
        roomId,
        eventTypes: [...eventTypes] // Convert readonly array
      };
      
      const serverResult = await this.remoteExecute(serverParams, 'chat/subscribe-room', 'server');
      
      if (serverResult && (serverResult as SubscribeRoomResult).success) {
        console.log(`✅ Browser: Successfully subscribed to room "${roomId}" via server`);
        return (serverResult as SubscribeRoomResult).subscriptionId!;
      } else {
        throw new Error(`Server subscription failed: ${(serverResult as SubscribeRoomResult)?.error || 'Unknown error'}`);
      }
      
    } catch (error) {
      console.error(`❌ Browser: Failed to subscribe to room events:`, error);
      throw error;
    }
  }

  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }
}