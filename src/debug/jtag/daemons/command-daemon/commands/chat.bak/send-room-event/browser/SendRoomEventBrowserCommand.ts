import { SendRoomEventCommand } from '../shared/SendRoomEventCommand';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';
import type { ICommandDaemon } from '../../../shared/CommandBase';
import { SendRoomEventParams, SendRoomEventResult } from '../shared/SendRoomEventTypes';

export class SendRoomEventBrowserCommand extends SendRoomEventCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('send-room-event', context, subpath, commander);
  }

  async execute(params: SendRoomEventParams): Promise<SendRoomEventResult> {
    const eventParams = params;
    
    console.log(`📤 BROWSER: Sending ${eventParams.eventType} event to room ${eventParams.roomId}`);

    try {
      // Update local UI optimistically if applicable
      await this.updateLocalUI(eventParams);

      // Route to server for actual event processing
      const result = await this.commander.routeToServer({
        command: 'chat/send-room-event',
        params: eventParams
      });

      // Handle successful event send
      if (result.success) {
        console.log(`✅ BROWSER: Room event sent successfully: ${result.eventId}`);
        await this.handleSuccessfulEventSend(result);
      } else {
        console.error(`❌ BROWSER: Room event send failed: ${result.error}`);
        await this.handleFailedEventSend(eventParams, result.error);
      }

      return result;

    } catch (error: any) {
      console.error(`❌ BROWSER: Send room event error:`, error.message);
      await this.handleFailedEventSend(eventParams, error.message);
      
      return new SendRoomEventResult({
        roomId: eventParams.roomId,
        success: false,
        error: error.message
      });
    }
  }

  /**
   * Update local UI optimistically before server confirmation
   */
  private async updateLocalUI(params: SendRoomEventParams): Promise<void> {
    try {
      switch (params.eventType) {
        case 'widget_state_changed':
          await this.updateWidgetUI(params);
          break;
        case 'typing_started':
        case 'typing_stopped':
          await this.updateTypingIndicator(params);
          break;
        case 'participant_status_changed':
          await this.updateParticipantStatus(params);
          break;
        case 'cursor_moved':
        case 'viewport_changed':
          await this.updateCollaborativeElements(params);
          break;
        default:
          // No specific UI update needed
          break;
      }
    } catch (error) {
      console.warn(`⚠️ BROWSER: Local UI update failed:`, error);
    }
  }

  /**
   * Update widget UI for widget events
   */
  private async updateWidgetUI(params: SendRoomEventParams): Promise<void> {
    if (params.widgetEventOptions) {
      const widget = document.querySelector(`[widget-id="${params.sourceParticipantId}"]`);
      if (widget && (widget as any).updateState) {
        await (widget as any).updateState({
          newState: params.widgetEventOptions.stateSnapshot,
          stateDelta: params.widgetEventOptions.stateDelta,
          optimistic: true
        });
      }
    }
  }

  /**
   * Update typing indicators
   */
  private async updateTypingIndicator(params: SendRoomEventParams): Promise<void> {
    const chatWidget = document.querySelector('chat-widget');
    if (chatWidget && (chatWidget as any).updateTypingIndicator) {
      await (chatWidget as any).updateTypingIndicator({
        participantId: params.sourceParticipantId,
        isTyping: params.eventType === 'typing_started',
        timestamp: Date.now()
      });
    }
  }

  /**
   * Update participant status display
   */
  private async updateParticipantStatus(params: SendRoomEventParams): Promise<void> {
    const participantList = document.querySelector('.participant-list');
    if (participantList && (participantList as any).updateStatus) {
      await (participantList as any).updateStatus({
        participantId: params.sourceParticipantId,
        newStatus: params.eventData.statusChange,
        optimistic: true
      });
    }
  }

  /**
   * Update collaborative elements (cursors, viewports)
   */
  private async updateCollaborativeElements(params: SendRoomEventParams): Promise<void> {
    const collaborationLayer = document.querySelector('.collaboration-layer');
    if (collaborationLayer && (collaborationLayer as any).updateElement) {
      await (collaborationLayer as any).updateElement({
        participantId: params.sourceParticipantId,
        eventType: params.eventType,
        eventData: params.eventData,
        optimistic: true
      });
    }
  }

  /**
   * Handle successful event send
   */
  private async handleSuccessfulEventSend(result: SendRoomEventResult): Promise<void> {
    try {
      // Confirm optimistic updates
      const confirmEvent = new CustomEvent('room-event-confirmed', {
        detail: {
          eventId: result.eventId,
          roomId: result.roomId,
          recipientCount: result.recipientCount,
          deliveryTime: result.deliveryTime
        }
      });
      document.dispatchEvent(confirmEvent);

      // Update UI with server response data
      if (result.widgetCoordinationResults) {
        await this.handleWidgetCoordinationResults(result.widgetCoordinationResults);
      }

      if (result.academyIntegrationResults) {
        await this.handleAcademyIntegrationResults(result.academyIntegrationResults);
      }

      // Show success feedback if appropriate
      if (result.eventImpact && result.eventImpact.significanceLevel !== 'minor') {
        await this.showEventImpactFeedback(result.eventImpact);
      }

    } catch (error) {
      console.warn(`⚠️ BROWSER: Success handling failed:`, error);
    }
  }

  /**
   * Handle failed event send
   */
  private async handleFailedEventSend(params: SendRoomEventParams, error: string): Promise<void> {
    try {
      // Revert optimistic updates
      const revertEvent = new CustomEvent('room-event-failed', {
        detail: {
          eventType: params.eventType,
          roomId: params.roomId,
          sourceParticipantId: params.sourceParticipantId,
          error: error
        }
      });
      document.dispatchEvent(revertEvent);

      // Show error notification
      await this.showErrorNotification(`Failed to send ${params.eventType} event: ${error}`);

    } catch (error) {
      console.warn(`⚠️ BROWSER: Error handling failed:`, error);
    }
  }

  /**
   * Handle widget coordination results
   */
  private async handleWidgetCoordinationResults(results: any): Promise<void> {
    try {
      const widgetSystem = document.querySelector('widget-coordinator');
      if (widgetSystem && (widgetSystem as any).handleCoordinationResults) {
        await (widgetSystem as any).handleCoordinationResults(results);
      }
    } catch (error) {
      console.warn(`⚠️ BROWSER: Widget coordination handling failed:`, error);
    }
  }

  /**
   * Handle Academy integration results
   */
  private async handleAcademyIntegrationResults(results: any): Promise<void> {
    try {
      const academyWidget = document.querySelector('academy-widget');
      if (academyWidget && (academyWidget as any).handleIntegrationResults) {
        await (academyWidget as any).handleIntegrationResults(results);
      }
    } catch (error) {
      console.warn(`⚠️ BROWSER: Academy integration handling failed:`, error);
    }
  }

  /**
   * Show event impact feedback
   */
  private async showEventImpactFeedback(impact: any): Promise<void> {
    try {
      const notification = {
        type: 'impact',
        message: `Event had ${impact.significanceLevel} impact on ${impact.participantsReached} participants`,
        details: impact,
        duration: 3000
      };

      const notificationSystem = document.querySelector('.notification-system');
      if (notificationSystem && (notificationSystem as any).showNotification) {
        await (notificationSystem as any).showNotification(notification);
      }
    } catch (error) {
      console.warn(`⚠️ BROWSER: Impact feedback failed:`, error);
    }
  }

  /**
   * Show error notification
   */
  private async showErrorNotification(message: string): Promise<void> {
    try {
      const notificationSystem = document.querySelector('.notification-system');
      if (notificationSystem && (notificationSystem as any).showError) {
        await (notificationSystem as any).showError(message);
      } else {
        console.error(`Room Event Error: ${message}`);
      }
    } catch (error) {
      console.warn(`⚠️ BROWSER: Error notification failed:`, error);
    }
  }
}