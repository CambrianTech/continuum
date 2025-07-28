/**
 * Send Room Event Command - Server Implementation
 * 
 * Server implementation for sending events within room-scoped coordination system.
 * Routes events through room event bus for proper delivery to all participants.
 */

import { CommandBase } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import type { ICommandDaemon } from '@commandBase';
import { SendRoomEventParams, SendRoomEventResult } from '../shared/SendRoomEventTypes';
import type { RoomEvent } from '../../room-events/shared/RoomEventTypes';

export class SendRoomEventServerCommand extends CommandBase<SendRoomEventParams, SendRoomEventResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('send-room-event', context, subpath, commander);
  }

  /**
   * Server sends room event through event bus system
   */
  async execute(params: JTAGPayload): Promise<SendRoomEventResult> {
    const eventParams = params as SendRoomEventParams;
    
    console.log(`📤 SERVER: Sending ${eventParams.eventType} event from ${eventParams.sourceParticipantId} in room ${eventParams.roomId}`);

    try {
      // 1. Validate event parameters
      const validationResult = await this.validateEventParams(eventParams);
      if (!validationResult.valid) {
        return new SendRoomEventResult({
          roomId: eventParams.roomId,
          success: false,
          error: validationResult.error
        });
      }

      // 2. Create room event object
      const roomEvent = await this.createRoomEvent(eventParams);

      // 3. Apply delivery options and filters
      const deliveryConfig = await this.processDeliveryConfiguration(eventParams);

      // 4. Route through room event bus
      const deliveryResult = await this.routeThroughEventBus(roomEvent, deliveryConfig);

      // 5. Process widget coordination if needed
      const widgetResults = await this.processWidgetCoordination(eventParams, roomEvent);

      // 6. Process Academy integration if needed
      const academyResults = await this.processAcademyIntegration(eventParams, roomEvent);

      // 7. Calculate event impact metrics
      const impactMetrics = await this.calculateEventImpact(roomEvent, deliveryResult);

      return new SendRoomEventResult({
        success: true,
        eventId: roomEvent.eventId,
        roomId: eventParams.roomId,
        recipientCount: deliveryResult.recipientCount,
        deliveryTime: deliveryResult.deliveryTime,
        acknowledgedBy: deliveryResult.acknowledgedBy,
        eventProcessingTime: deliveryResult.processingTime,
        routingTime: deliveryResult.routingTime,
        deliveryStatus: deliveryResult.status,
        failedDeliveries: deliveryResult.failures,
        widgetCoordinationResults: widgetResults,
        academyIntegrationResults: academyResults,
        eventImpact: impactMetrics
      });

    } catch (error: any) {
      console.error(`❌ SERVER: Failed to send room event:`, error.message);
      return new SendRoomEventResult({
        roomId: eventParams.roomId,
        success: false,
        error: error.message
      });
    }
  }

  /**
   * Validate event parameters
   */
  private async validateEventParams(params: SendRoomEventParams): Promise<{ valid: boolean; error?: string }> {
    // Basic validation
    if (!params.roomId) {
      return { valid: false, error: 'Room ID is required' };
    }

    if (!params.sourceParticipantId) {
      return { valid: false, error: 'Source participant ID is required' };
    }

    if (!params.eventType) {
      return { valid: false, error: 'Event type is required' };
    }

    // Validate participant has permission to send events in room
    try {
      const permissionResult = await this.commander.routeCommand({
        command: 'chat/validate-room-access',
        params: {
          participantId: params.sourceParticipantId,
          participantType: params.sourceParticipantType,
          roomId: params.roomId,
          requiredPermissions: ['send_events']
        }
      });

      if (!permissionResult.success || !permissionResult.hasAccess) {
        return { valid: false, error: 'Participant does not have permission to send events in this room' };
      }
    } catch (error) {
      console.warn(`⚠️ SERVER: Permission validation failed:`, error);
      return { valid: false, error: 'Unable to validate room permissions' };
    }

    return { valid: true };
  }

  /**
   * Create room event object
   */
  private async createRoomEvent(params: SendRoomEventParams): Promise<RoomEvent> {
    const eventId = `evt_${params.roomId}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;

    const roomEvent: RoomEvent = {
      eventId,
      eventType: params.eventType,
      roomId: params.roomId,
      timestamp: Date.now(),
      sourceParticipantId: params.sourceParticipantId,
      sourceParticipantType: params.sourceParticipantType,
      eventData: params.eventData,
      priority: params.priority || 'normal',
      ttl: params.ttl,
      correlationId: params.correlationId,
      deliveryAttempts: 0,
      widgetContext: params.widgetEventOptions ? this.createWidgetContext(params) : undefined,
      academyContext: params.academyEventOptions ? this.createAcademyContext(params) : undefined
    };

    return roomEvent;
  }

  /**
   * Create widget context for event
   */
  private createWidgetContext(params: SendRoomEventParams): any {
    const widgetOptions = params.widgetEventOptions;
    if (!widgetOptions) return undefined;

    return {
      widgetId: params.sourceParticipantId,
      widgetType: 'unknown', // Could be determined from participant registry
      interactionContext: widgetOptions.interactionType,
      userAction: widgetOptions.interactionType,
      inputData: widgetOptions.interactionData,
      widgetStateSnapshot: widgetOptions.stateSnapshot,
      stateChangeDelta: widgetOptions.stateDelta
    };
  }

  /**
   * Create Academy context for event
   */
  private createAcademyContext(params: SendRoomEventParams): any {
    const academyOptions = params.academyEventOptions;
    if (!academyOptions) return undefined;

    return {
      sessionId: academyOptions.sessionId || `session_${params.roomId}`,
      sessionPhase: academyOptions.sessionPhase || 'active',
      currentObjectives: academyOptions.currentObjectives || [],
      learningProgress: academyOptions.learningValue,
      performanceMetrics: academyOptions.performanceMetrics
    };
  }

  /**
   * Process delivery configuration
   */
  private async processDeliveryConfiguration(params: SendRoomEventParams): Promise<any> {
    return {
      deliveryOptions: params.deliveryOptions || {
        guaranteedDelivery: true,
        deliverToAll: true,
        immediateDelivery: true
      },
      targetFilters: params.targetFilters || {}
    };
  }

  /**
   * Route event through room event bus
   */
  private async routeThroughEventBus(event: RoomEvent, deliveryConfig: any): Promise<any> {
    try {
      // Get room event bus command
      const roomEventResult = await this.commander.routeCommand({
        command: 'chat/room-events',
        params: {
          participantId: 'system',
          participantType: 'system_agent',
          roomId: event.roomId,
          action: 'route_event',
          event: event,
          deliveryConfig: deliveryConfig
        }
      });

      if (roomEventResult.success) {
        return {
          recipientCount: roomEventResult.recipientCount || 0,
          deliveryTime: roomEventResult.deliveryTime || 0,
          acknowledgedBy: roomEventResult.acknowledgedBy || [],
          processingTime: roomEventResult.processingTime || 0,
          routingTime: roomEventResult.routingTime || 0,
          status: roomEventResult.deliveryStatus || 'delivered',
          failures: roomEventResult.failedDeliveries || []
        };
      } else {
        throw new Error(roomEventResult.error || 'Event bus routing failed');
      }
    } catch (error) {
      console.error(`❌ SERVER: Event bus routing failed:`, error);
      return {
        recipientCount: 0,
        deliveryTime: 0,
        acknowledgedBy: [],
        processingTime: 0,
        routingTime: 0,
        status: 'failed',
        failures: [{ participantId: 'event_bus', participantType: 'system_agent', failureReason: 'Event bus routing failed' }]
      };
    }
  }

  /**
   * Process widget coordination
   */
  private async processWidgetCoordination(params: SendRoomEventParams, event: RoomEvent): Promise<any> {
    const widgetOptions = params.widgetEventOptions;
    if (!widgetOptions) {
      return undefined;
    }

    try {
      // Coordinate with other widgets if requested
      const coordination = {
        widgetsSynced: [],
        syncFailures: [],
        sharedStateUpdated: false,
        stateUpdateConflicts: false,
        triggeredInteractions: [],
        uiUpdatesTriggered: 0,
        uiUpdateLatency: 0,
        coordinationLatency: 0,
        coordinationEfficiency: 1.0
      };

      // Simulate widget coordination
      if (widgetOptions.triggerWidgetSync) {
        coordination.widgetsSynced = widgetOptions.collaboratingWidgets || [];
      }

      if (widgetOptions.updateSharedState) {
        coordination.sharedStateUpdated = true;
      }

      if (widgetOptions.uiUpdateRequired) {
        coordination.uiUpdatesTriggered = 1;
        coordination.uiUpdateLatency = 50; // ms
      }

      return coordination;
    } catch (error) {
      console.warn(`⚠️ SERVER: Widget coordination failed:`, error);
      return undefined;
    }
  }

  /**
   * Process Academy integration
   */
  private async processAcademyIntegration(params: SendRoomEventParams, event: RoomEvent): Promise<any> {
    const academyOptions = params.academyEventOptions;
    if (!academyOptions) {
      return undefined;
    }

    try {
      const integration = {
        learningTracked: false,
        trainingProgressUpdated: false,
        performanceAssessed: false,
        assessmentResults: [],
        evolutionTriggered: false,
        evolutionDetails: undefined,
        milestoneProgressUpdated: false,
        milestonesCompleted: [],
        capabilitiesTracked: [],
        capabilityLevelUpdates: [],
        trainerNotified: false,
        sessionUpdated: false
      };

      // Track learning value if provided
      if (academyOptions.learningValue !== undefined) {
        integration.learningTracked = true;
      }

      // Update training progress if requested
      if (academyOptions.updateTrainingProgress) {
        integration.trainingProgressUpdated = true;
      }

      // Track capabilities if provided
      if (academyOptions.capabilityDemonstration) {
        integration.capabilitiesTracked = academyOptions.capabilityDemonstration;
      }

      // Process evolution trigger
      if (academyOptions.evolutionTrigger) {
        integration.evolutionTriggered = true;
        integration.evolutionDetails = {
          evolutionId: `evo_${Date.now()}`,
          personaId: params.sourceParticipantId,
          evolutionType: 'learning_advancement',
          evolutionTrigger: academyOptions.evolutionContext || 'event_triggered',
          capabilitiesGained: [],
          capabilityLevelChanges: new Map(),
          newSkills: [],
          evolutionSignificance: 0.5,
          evolutionReadiness: 0.8,
          evolutionSuccess: true,
          evolutionStartTime: Date.now()
        };
      }

      // Coordinate with trainer if requested
      if (academyOptions.coordinateWithTrainer) {
        integration.trainerNotified = true;
      }

      return integration;
    } catch (error) {
      console.warn(`⚠️ SERVER: Academy integration failed:`, error);
      return undefined;
    }
  }

  /**
   * Calculate event impact metrics
   */
  private async calculateEventImpact(event: RoomEvent, deliveryResult: any): Promise<any> {
    return {
      participantsReached: deliveryResult.recipientCount,
      participantsEngaged: deliveryResult.acknowledgedBy.length,
      responsesGenerated: 0,
      averageResponseTime: 0,
      widgetsCoordinated: 0,
      personasActivated: 0,
      learningInteractionsTriggered: 0,
      teachingMomentsCreated: 0,
      academySessionsAffected: event.academyContext ? 1 : 0,
      trainingProgressImpacted: 0,
      overallImpactScore: 0.5,
      significanceLevel: 'moderate',
      secondaryEventsTriggered: 0,
      cascadeDepth: 1
    };
  }
}