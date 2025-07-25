import { RoomEventCommand } from '../shared/RoomEventCommand';
import { RoomEventSubscriptionParams, RoomEventSubscriptionResult } from '../shared/RoomEventTypes';
import type { ICommandDaemon } from '../../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../../shared/JTAGTypes';

export class RoomEventBrowserCommand extends RoomEventCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: RoomEventSubscriptionParams): Promise<RoomEventSubscriptionResult> {
    try {
      console.log(`📡 BROWSER: Setting up room event subscription for ${params.participantType} ${params.participantId}`);

      // Route to server for actual subscription setup
      const result = await this.commander.routeToServer({
        command: 'chat/room-events',
        params: params
      });

      // Set up browser-side event listening if subscription successful
      if (result.success && result.eventStreamEndpoint) {
        await this.setupBrowserEventStream(result);
      }

      return result;

    } catch (error) {
      console.error(`❌ BROWSER: Room event subscription error:`, error);
      return new RoomEventSubscriptionResult({
        participantId: params.participantId,
        roomId: params.roomId,
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }

  private async setupBrowserEventStream(subscriptionResult: RoomEventSubscriptionResult): Promise<void> {
    try {
      // Set up WebSocket or SSE connection for event stream
      if (subscriptionResult.eventStreamEndpoint) {
        const eventSource = new EventSource(subscriptionResult.eventStreamEndpoint);
        
        eventSource.onmessage = (event) => {
          try {
            const roomEvent = JSON.parse(event.data);
            this.handleRoomEvent(roomEvent);
          } catch (error) {
            console.error(`❌ BROWSER: Failed to parse room event:`, error);
          }
        };

        eventSource.onerror = (error) => {
          console.error(`❌ BROWSER: Event stream error:`, error);
        };

        // Store event source for cleanup
        (window as any).roomEventSources = (window as any).roomEventSources || new Map();
        (window as any).roomEventSources.set(subscriptionResult.subscriptionId, eventSource);
      }
    } catch (error) {
      console.error(`❌ BROWSER: Event stream setup failed:`, error);
    }
  }

  private handleRoomEvent(roomEvent: any): void {
    console.log(`📨 BROWSER: Received room event: ${roomEvent.eventType} in room ${roomEvent.roomId}`);

    // Route event to appropriate handlers
    switch (roomEvent.eventType) {
      case 'message_sent':
        this.handleMessageEvent(roomEvent);
        break;
      case 'participant_joined':
      case 'participant_left':
        this.handleParticipantEvent(roomEvent);
        break;
      case 'widget_activated':
      case 'widget_state_changed':
        this.handleWidgetEvent(roomEvent);
        break;
      case 'academy_phase_changed':
      case 'capability_demonstrated':
        this.handleAcademyEvent(roomEvent);
        break;
      default:
        this.handleGenericEvent(roomEvent);
    }
  }

  private handleMessageEvent(roomEvent: any): void {
    // Update chat widget with new message
    const chatWidget = document.querySelector('chat-widget');
    if (chatWidget && (chatWidget as any).addMessage) {
      (chatWidget as any).addMessage({
        id: roomEvent.eventData.messageId,
        roomId: roomEvent.roomId,
        content: roomEvent.eventData.messageContent,
        senderId: roomEvent.sourceParticipantId,
        senderType: roomEvent.sourceParticipantType,
        messageType: roomEvent.eventData.messageType,
        timestamp: roomEvent.timestamp
      });
    }
  }

  private handleParticipantEvent(roomEvent: any): void {
    // Update participant list in UI
    const participantList = document.querySelector('.participant-list');
    if (participantList && (participantList as any).updateParticipant) {
      (participantList as any).updateParticipant({
        participantId: roomEvent.eventData.participantId,
        action: roomEvent.eventType === 'participant_joined' ? 'join' : 'leave',
        timestamp: roomEvent.timestamp
      });
    }
  }

  private handleWidgetEvent(roomEvent: any): void {
    // Route to widget coordination system
    const widgetSystem = document.querySelector('widget-coordinator');
    if (widgetSystem && (widgetSystem as any).handleWidgetEvent) {
      (widgetSystem as any).handleWidgetEvent(roomEvent);
    }
  }

  private handleAcademyEvent(roomEvent: any): void {
    // Route to Academy UI components
    const academyWidget = document.querySelector('academy-widget');
    if (academyWidget && (academyWidget as any).handleAcademyEvent) {
      (academyWidget as any).handleAcademyEvent(roomEvent);
    }
  }

  private handleGenericEvent(roomEvent: any): void {
    // Generic event handling - dispatch to any interested components
    const customEvent = new CustomEvent('room-event', {
      detail: roomEvent
    });
    document.dispatchEvent(customEvent);
  }
}