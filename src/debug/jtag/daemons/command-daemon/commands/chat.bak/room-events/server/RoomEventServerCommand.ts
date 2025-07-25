/**
 * Room Event Command - Server Implementation
 * 
 * Server implementation handles:
 * 1. Room-scoped event bus management
 * 2. Participant subscription management
 * 3. Event routing within room boundaries
 * 4. Widget-persona-human coordination
 */

import { CommandBase } from '../../../shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../shared/JTAGTypes';
import type { 
  RoomEventSubscriptionParams, 
  RoomEvent, 
  RoomEventBusState, 
  RoomEventSubscription,
  WidgetEventCoordination 
} from '../shared/RoomEventTypes';
import { RoomEventSubscriptionResult } from '../shared/RoomEventTypes';
import type { ICommandDaemon } from '../../../shared/CommandBase';

export class RoomEventServerCommand extends CommandBase<RoomEventSubscriptionParams, RoomEventSubscriptionResult> {
  
  // Room-scoped event buses (one per room)
  private roomEventBuses = new Map<string, RoomEventBusState>();
  
  // Widget coordination per room
  private roomWidgetCoordination = new Map<string, WidgetEventCoordination>();
  
  // Global subscription tracking
  private subscriptionRegistry = new Map<string, RoomEventSubscription>();

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('room-events', context, subpath, commander);
    this.initializeEventBusManagement();
  }

  /**
   * Server establishes room event subscription for participant
   */
  async execute(params: JTAGPayload): Promise<RoomEventSubscriptionResult> {
    const subscriptionParams = params as RoomEventSubscriptionParams;
    
    console.log(`📡 SERVER: Setting up room event subscription for ${subscriptionParams.participantType} ${subscriptionParams.participantId} in room ${subscriptionParams.roomId}`);

    try {
      // 1. Ensure room event bus exists
      await this.ensureRoomEventBus(subscriptionParams.roomId);
      
      // 2. Validate participant has access to room
      const hasAccess = await this.validateRoomAccess(subscriptionParams);
      if (!hasAccess) {
        return new RoomEventSubscriptionResult({
          success: false,
          participantId: subscriptionParams.participantId,
          roomId: subscriptionParams.roomId,
          subscribedEventTypes: [],
          subscriptionStatus: 'error',
          error: 'Participant does not have access to this room'
        });
      }

      // 3. Create subscription
      const subscription = await this.createRoomSubscription(subscriptionParams);
      
      // 4. Register with room event bus
      await this.registerWithRoomEventBus(subscriptionParams.roomId, subscription);
      
      // 5. Set up widget coordination if participant is a widget
      if (subscriptionParams.participantType === 'widget') {
        await this.setupWidgetCoordination(subscriptionParams, subscription);
      }
      
      // 6. Set up persona coordination if participant is a persona
      if (subscriptionParams.participantType === 'persona') {
        await this.setupPersonaCoordination(subscriptionParams, subscription);
      }
      
      // 7. Provide backfill events if requested
      let backfilledEvents: RoomEvent[] = [];
      if (subscriptionParams.subscriptionOptions?.backfillOnSubscribe) {
        backfilledEvents = await this.getBackfillEvents(subscriptionParams);
      }

      return new RoomEventSubscriptionResult({
        success: true,
        subscriptionId: subscription.subscriptionId,
        participantId: subscriptionParams.participantId,
        roomId: subscriptionParams.roomId,
        subscribedEventTypes: Array.from(subscription.eventTypes),
        subscriptionStatus: 'active',
        eventStreamEndpoint: this.generateEventStreamEndpoint(subscription),
        eventStreamToken: this.generateEventStreamToken(subscription),
        backfilledEvents,
        backfillCount: backfilledEvents.length,
        subscriptionLatency: Date.now() - this.startTime,
        expectedEventRate: this.calculateExpectedEventRate(subscriptionParams.roomId)
      });

    } catch (error: any) {
      console.error(`❌ SERVER: Failed to setup room event subscription:`, error.message);
      return new RoomEventSubscriptionResult({
        success: false,
        participantId: subscriptionParams.participantId,
        roomId: subscriptionParams.roomId,
        subscribedEventTypes: [],
        subscriptionStatus: 'error',
        error: error.message
      });
    }
  }

  /**
   * Initialize event bus management system
   */
  private initializeEventBusManagement(): void {
    // Start background tasks
    this.startHealthMonitoring();
    this.startPerformanceMonitoring();
    this.startCleanupTasks();
    
    console.log(`🚌 SERVER: Room event bus management initialized`);
  }

  /**
   * Ensure room event bus exists
   */
  private async ensureRoomEventBus(roomId: string): Promise<void> {
    if (!this.roomEventBuses.has(roomId)) {
      const eventBus: RoomEventBusState = {
        roomId,
        subscribers: new Map(),
        activeSubscribers: 0,
        eventsSent: 0,
        eventsDelivered: 0,
        eventsFailed: 0,
        averageDeliveryTime: 0,
        busHealth: {
          healthScore: 1.0,
          connectionHealth: 1.0,
          deliveryHealth: 1.0,
          performanceHealth: 1.0,
          activeIssues: [],
          warningCount: 0,
          errorCount: 0,
          lastHealthCheck: Date.now(),
          recoveryAttempts: 0
        },
        throughputMetrics: {
          eventsPerSecond: 0,
          peakEventsPerSecond: 0,
          averageEventSize: 0,
          maxSubscribers: 1000,
          currentSubscribers: 0,
          maxEventsPerSecond: 100,
          averageLatency: 0,
          deliverySuccessRate: 1.0,
          memoryUsage: 0,
          cpuUsage: 0,
          networkBandwidth: 0
        },
        busConfig: {
          maxSubscribers: 1000,
          maxEventsPerSecond: 100,
          maxEventSize: 1024 * 1024, // 1MB
          maxEventQueueSize: 10000,
          batchingEnabled: true,
          batchSize: 10,
          batchingDelay: 100,
          compressionEnabled: true,
          retryAttempts: 3,
          retryDelay: 1000,
          duplicateDetection: true,
          orderPreservation: true,
          subscriptionTTL: 24 * 60 * 60 * 1000, // 24 hours
          eventTTL: 60 * 60 * 1000, // 1 hour
          cleanupInterval: 5 * 60 * 1000, // 5 minutes
          healthCheckInterval: 30 * 1000, // 30 seconds
          performanceMonitoring: true,
          alertingEnabled: true
        }
      };

      this.roomEventBuses.set(roomId, eventBus);
      
      // Initialize widget coordination for room
      this.roomWidgetCoordination.set(roomId, {
        activeWidgets: new Map(),
        widgetSyncEvents: [],
        collaborativeWidgets: [],
        widgetInteractions: [],
        sharedWidgetState: new Map(),
        widgetStateHistory: []
      });

      console.log(`🚌 SERVER: Created event bus for room ${roomId}`);
    }
  }

  /**
   * Validate participant has access to room
   */
  private async validateRoomAccess(params: RoomEventSubscriptionParams): Promise<boolean> {
    try {
      const accessResult = await this.commander.routeCommand({
        command: 'chat/validate-room-access',
        params: {
          participantId: params.participantId,
          participantType: params.participantType,
          roomId: params.roomId,
          requiredPermissions: ['subscribe_to_events']
        }
      });

      return accessResult.success && accessResult.hasAccess;
    } catch (error) {
      console.warn(`⚠️ SERVER: Room access validation failed:`, error);
      return false;
    }
  }

  /**
   * Create room subscription
   */
  private async createRoomSubscription(params: RoomEventSubscriptionParams): Promise<RoomEventSubscription> {
    const subscriptionId = `sub_${params.roomId}_${params.participantId}_${Date.now()}`;
    
    const subscription: RoomEventSubscription = {
      subscriptionId,
      participantId: params.participantId,
      participantType: params.participantType,
      eventTypes: new Set(params.eventTypes || []),
      filters: params.eventFilters || {},
      options: params.subscriptionOptions || {},
      status: 'active',
      createdAt: Date.now(),
      lastActivity: Date.now(),
      lastEventDelivered: 0,
      eventsReceived: 0,
      eventsFiltered: 0,
      averageProcessingTime: 0,
      connectionInfo: {
        connectionType: 'websocket', // Default to WebSocket
        connectionId: `conn_${subscriptionId}`,
        isConnected: true,
        connectionQuality: 1.0
      }
    };

    // Store in global registry
    this.subscriptionRegistry.set(subscriptionId, subscription);

    console.log(`📝 SERVER: Created subscription ${subscriptionId} for ${params.participantType} ${params.participantId}`);
    
    return subscription;
  }

  /**
   * Register subscription with room event bus
   */
  private async registerWithRoomEventBus(roomId: string, subscription: RoomEventSubscription): Promise<void> {
    const eventBus = this.roomEventBuses.get(roomId);
    if (!eventBus) {
      throw new Error(`Event bus not found for room ${roomId}`);
    }

    // Add to room event bus
    eventBus.subscribers.set(subscription.subscriptionId, subscription);
    eventBus.activeSubscribers++;
    eventBus.throughputMetrics.currentSubscribers++;

    console.log(`🔗 SERVER: Registered subscription ${subscription.subscriptionId} with room ${roomId} event bus`);
  }

  /**
   * Setup widget coordination
   */
  private async setupWidgetCoordination(params: RoomEventSubscriptionParams, subscription: RoomEventSubscription): Promise<void> {
    if (params.participantType !== 'widget') return;

    const widgetCoordination = this.roomWidgetCoordination.get(params.roomId);
    if (!widgetCoordination) return;

    try {
      // Get widget information
      const widgetInfo = await this.getWidgetInfo(params.participantId);
      
      const widgetParticipant = {
        widgetId: params.participantId,
        widgetType: widgetInfo.widgetType || 'unknown',
        roomId: params.roomId,
        isInteractive: widgetInfo.isInteractive || false,
        canCoordinate: widgetInfo.canCoordinate || false,
        canShareState: widgetInfo.canShareState || false,
        eventCapabilities: widgetInfo.eventCapabilities || {
          canSendEvents: [],
          subscribesToEvents: Array.from(subscription.eventTypes),
          canCoordinateWithOtherWidgets: false,
          canShareStateWithWidgets: false,
          canSynchronizeWithPersonas: false,
          capturesUIEvents: false,
          generatesUIEvents: false,
          canControlOtherWidgets: false
        },
        currentState: widgetInfo.initialState || {},
        lastActivity: Date.now(),
        responseTime: 0,
        eventProcessingRate: 0
      };

      widgetCoordination.activeWidgets.set(params.participantId, widgetParticipant);

      console.log(`🎛️ SERVER: Setup widget coordination for ${params.participantId} in room ${params.roomId}`);
      
    } catch (error) {
      console.warn(`⚠️ SERVER: Widget coordination setup failed:`, error);
    }
  }

  /**
   * Setup persona coordination
   */
  private async setupPersonaCoordination(params: RoomEventSubscriptionParams, subscription: RoomEventSubscription): Promise<void> {
    if (params.participantType !== 'persona') return;

    try {
      // Register persona for AI provider routing
      await this.commander.routeCommand({
        command: 'ai-provider/register-persona-event-subscription',
        params: {
          personaId: params.participantId,
          subscriptionId: subscription.subscriptionId,
          roomId: params.roomId,
          eventTypes: Array.from(subscription.eventTypes),
          eventRoutingEndpoint: `/room-events/${params.roomId}/persona/${params.participantId}`
        }
      });

      // Setup multi-context awareness integration
      await this.commander.routeCommand({
        command: 'chat/register-persona-room-context',
        params: {
          personaId: params.participantId,
          roomId: params.roomId,
          subscriptionId: subscription.subscriptionId,
          contextUpdateInterval: 10000 // 10 seconds
        }
      });

      console.log(`🤖 SERVER: Setup persona coordination for ${params.participantId} in room ${params.roomId}`);
      
    } catch (error) {
      console.warn(`⚠️ SERVER: Persona coordination setup failed:`, error);
    }
  }

  /**
   * Get backfill events for new subscription
   */
  private async getBackfillEvents(params: RoomEventSubscriptionParams): Promise<RoomEvent[]> {
    try {
      const backfillCount = params.subscriptionOptions?.backfillCount || 20;
      
      const backfillResult = await this.commander.routeCommand({
        command: 'chat/get-recent-room-events',
        params: {
          roomId: params.roomId,
          eventTypes: params.eventTypes,
          maxEvents: backfillCount,
          participantFilters: params.eventFilters
        }
      });

      if (backfillResult.success) {
        return backfillResult.events || [];
      }

      return [];
    } catch (error) {
      console.warn(`⚠️ SERVER: Backfill events failed:`, error);
      return [];
    }
  }

  /**
   * Route room event to subscribers
   */
  public async routeRoomEvent(roomId: string, event: RoomEvent): Promise<void> {
    const eventBus = this.roomEventBuses.get(roomId);
    if (!eventBus) {
      console.warn(`⚠️ SERVER: No event bus found for room ${roomId}`);
      return;
    }

    console.log(`📡 SERVER: Routing ${event.eventType} event to ${eventBus.activeSubscribers} subscribers in room ${roomId}`);

    const deliveryPromises: Promise<void>[] = [];

    // Route to each subscriber
    for (const [subscriptionId, subscription] of eventBus.subscribers) {
      if (this.shouldDeliverEvent(event, subscription)) {
        const deliveryPromise = this.deliverEventToSubscriber(event, subscription)
          .catch(error => {
            console.error(`❌ SERVER: Failed to deliver event ${event.eventId} to subscriber ${subscriptionId}:`, error);
            eventBus.eventsFailed++;
          });
        
        deliveryPromises.push(deliveryPromise);
      } else {
        subscription.eventsFiltered++;
      }
    }

    // Wait for all deliveries
    await Promise.all(deliveryPromises);

    // Update metrics
    eventBus.eventsSent++;
    eventBus.eventsDelivered += deliveryPromises.length;
  }

  /**
   * Check if event should be delivered to subscriber
   */
  private shouldDeliverEvent(event: RoomEvent, subscription: RoomEventSubscription): boolean {
    // Check event type filter
    if (subscription.eventTypes.size > 0 && !subscription.eventTypes.has(event.eventType)) {
      return false;
    }

    // Check priority filter
    if (subscription.filters.priorityThreshold) {
      const priorityLevels = { low: 0, normal: 1, high: 2, urgent: 3, immediate: 4 };
      if (priorityLevels[event.priority] < priorityLevels[subscription.filters.priorityThreshold]) {
        return false;
      }
    }

    // Check participant filters
    if (subscription.filters.excludeParticipants?.includes(event.sourceParticipantId)) {
      return false;
    }

    if (subscription.filters.includeParticipants && 
        !subscription.filters.includeParticipants.includes(event.sourceParticipantId)) {
      return false;
    }

    // Check participant type filters
    if (subscription.filters.excludeParticipantTypes?.includes(event.sourceParticipantType)) {
      return false;
    }

    if (subscription.filters.participantTypes && 
        !subscription.filters.participantTypes.includes(event.sourceParticipantType)) {
      return false;
    }

    return true;
  }

  /**
   * Deliver event to individual subscriber
   */
  private async deliverEventToSubscriber(event: RoomEvent, subscription: RoomEventSubscription): Promise<void> {
    const startTime = Date.now();

    try {
      // Route based on participant type
      switch (subscription.participantType) {
        case 'human':
          await this.deliverToHuman(event, subscription);
          break;
        case 'persona':
          await this.deliverToPersona(event, subscription);
          break;
        case 'widget':
          await this.deliverToWidget(event, subscription);
          break;
        case 'system_agent':
          await this.deliverToSystemAgent(event, subscription);
          break;
        default:
          await this.deliverGeneric(event, subscription);
      }

      // Update metrics
      const deliveryTime = Date.now() - startTime;
      subscription.eventsReceived++;
      subscription.averageProcessingTime = 
        (subscription.averageProcessingTime + deliveryTime) / 2;
      subscription.lastEventDelivered = Date.now();
      subscription.lastActivity = Date.now();

    } catch (error) {
      console.error(`❌ SERVER: Event delivery failed to ${subscription.participantId}:`, error);
      throw error;
    }
  }

  /**
   * Deliver event to human participant
   */
  private async deliverToHuman(event: RoomEvent, subscription: RoomEventSubscription): Promise<void> {
    // Route through WebSocket or SSE to browser
    await this.commander.routeCommand({
      command: 'websocket/send-to-user',
      params: {
        userId: subscription.participantId,
        messageType: 'room_event',
        data: {
          subscriptionId: subscription.subscriptionId,
          event: event
        }
      }
    });
  }

  /**
   * Deliver event to persona
   */
  private async deliverToPersona(event: RoomEvent, subscription: RoomEventSubscription): Promise<void> {
    // Route through AI provider daemon
    await this.commander.routeCommand({
      command: 'ai-provider/deliver-room-event',
      params: {
        personaId: subscription.participantId,
        subscriptionId: subscription.subscriptionId,
        event: event
      }
    });
  }

  /**
   * Deliver event to widget
   */
  private async deliverToWidget(event: RoomEvent, subscription: RoomEventSubscription): Promise<void> {
    // Route through browser widget system
    await this.commander.routeCommand({
      command: 'browser-widget/deliver-event',
      params: {
        widgetId: subscription.participantId,
        subscriptionId: subscription.subscriptionId,
        event: event
      }
    });
  }

  /**
   * Deliver event to system agent
   */
  private async deliverToSystemAgent(event: RoomEvent, subscription: RoomEventSubscription): Promise<void> {
    // Route to system agent endpoint
    await this.commander.routeCommand({
      command: 'system-agent/deliver-event',
      params: {
        agentId: subscription.participantId,
        subscriptionId: subscription.subscriptionId,
        event: event
      }
    });
  }

  /**
   * Generic event delivery
   */
  private async deliverGeneric(event: RoomEvent, subscription: RoomEventSubscription): Promise<void> {
    // Default delivery mechanism
    console.log(`📤 SERVER: Generic delivery of ${event.eventType} to ${subscription.participantId}`);
  }

  // Helper methods
  private generateEventStreamEndpoint(subscription: RoomEventSubscription): string {
    return `/api/room-events/stream/${subscription.subscriptionId}`;
  }

  private generateEventStreamToken(subscription: RoomEventSubscription): string {
    return `token_${subscription.subscriptionId}_${Date.now()}`;
  }

  private calculateExpectedEventRate(roomId: string): number {
    const eventBus = this.roomEventBuses.get(roomId);
    return eventBus?.throughputMetrics.eventsPerSecond || 1;
  }

  private async getWidgetInfo(widgetId: string): Promise<any> {
    // Get widget information from widget registry
    return {
      widgetType: 'unknown',
      isInteractive: false,
      canCoordinate: false,
      canShareState: false,
      eventCapabilities: {},
      initialState: {}
    };
  }

  private startHealthMonitoring(): void {
    setInterval(() => {
      for (const [roomId, eventBus] of this.roomEventBuses) {
        this.checkEventBusHealth(roomId, eventBus);
      }
    }, 30000); // Check every 30 seconds
  }

  private startPerformanceMonitoring(): void {
    setInterval(() => {
      for (const [roomId, eventBus] of this.roomEventBuses) {
        this.updatePerformanceMetrics(roomId, eventBus);
      }
    }, 10000); // Update every 10 seconds
  }

  private startCleanupTasks(): void {
    setInterval(() => {
      this.cleanupExpiredSubscriptions();
      this.cleanupOldEvents();
    }, 5 * 60 * 1000); // Cleanup every 5 minutes
  }

  private checkEventBusHealth(roomId: string, eventBus: RoomEventBusState): void {
    // Implementation would check event bus health
  }

  private updatePerformanceMetrics(roomId: string, eventBus: RoomEventBusState): void {
    // Implementation would update performance metrics
  }

  private cleanupExpiredSubscriptions(): void {
    // Implementation would cleanup expired subscriptions
  }

  private cleanupOldEvents(): void {
    // Implementation would cleanup old events
  }

  private startTime = Date.now();
}