/**
 * Receive Events Command - Server Implementation
 * 
 * Server implementation handles:
 * 1. Establishing persona event streams
 * 2. Routing events from chat daemon to personas
 * 3. Managing multi-context awareness
 * 4. Event filtering and processing
 */

import { CommandBase } from '../../../shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../shared/JTAGTypes';
import type { ReceiveEventsParams, ChatEvent, MultiContextState, EventResponse } from '../shared/ReceiveEventsTypes';
import { ReceiveEventsResult } from '../shared/ReceiveEventsTypes';
import type { ICommandDaemon } from '../../../shared/CommandBase';

export class ReceiveEventsServerCommand extends CommandBase<ReceiveEventsParams, ReceiveEventsResult> {
  
  // Active persona streams
  private personaStreams = new Map<string, PersonaStream>();
  
  // Multi-context state management
  private multiContextStates = new Map<string, MultiContextState>();

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('receive-events', context, subpath, commander);
    this.initializeEventRouting();
  }

  /**
   * Server establishes event stream for persona
   */
  async execute(params: JTAGPayload): Promise<ReceiveEventsResult> {
    const eventsParams = params as ReceiveEventsParams;
    
    console.log(`📡 SERVER: Setting up event stream for persona ${eventsParams.personaId}`);

    try {
      // 1. Validate persona exists and has permissions
      const personaValid = await this.validatePersona(eventsParams.personaId);
      if (!personaValid) {
        return new ReceiveEventsResult({
          success: false,
          personaId: eventsParams.personaId,
          connectionStatus: 'error',
          error: 'Persona not found or lacks permissions'
        });
      }

      // 2. Create event stream for persona
      const streamId = await this.createEventStream(eventsParams);
      
      // 3. Register with chat daemon for event routing
      await this.registerWithChatDaemon(eventsParams, streamId);
      
      // 4. Initialize multi-context awareness if enabled
      if (eventsParams.multiContextEnabled) {
        await this.initializeMultiContext(eventsParams.personaId);
      }
      
      // 5. Start event processing loop
      this.startEventProcessing(eventsParams.personaId, streamId);

      return new ReceiveEventsResult({
        success: true,
        streamId,
        personaId: eventsParams.personaId,
        connectionStatus: 'connected',
        streamEndpoint: `/stream/persona/${eventsParams.personaId}`,
        streamToken: this.generateStreamToken(eventsParams.personaId),
        appliedConfig: eventsParams.streamConfig,
        appliedFilters: eventsParams.eventFilters,
        connectionTime: Date.now()
      });

    } catch (error: any) {
      console.error(`❌ SERVER: Failed to setup event stream:`, error.message);
      return new ReceiveEventsResult({
        success: false,
        personaId: eventsParams.personaId,
        connectionStatus: 'error',
        error: error.message
      });
    }
  }

  /**
   * Initialize event routing system
   */
  private initializeEventRouting(): void {
    // Set up event handlers for different event types
    this.setupEventHandlers();
    
    // Start background tasks
    this.startHealthChecking();
    this.startContextRefreshing();
  }

  /**
   * Validate persona exists and has permissions
   */
  private async validatePersona(personaId: string): Promise<boolean> {
    try {
      const validationResult = await this.commander.routeCommand({
        command: 'chat/validate-citizen',
        params: {
          citizenId: personaId,
          requiredCapabilities: ['send_messages', 'receive_events', 'multi_room_participation']
        }
      });

      return validationResult.success && validationResult.isValid;
    } catch (error) {
      console.warn(`⚠️ SERVER: Persona validation failed:`, error);
      return false;
    }
  }

  /**
   * Create event stream for persona
   */
  private async createEventStream(params: ReceiveEventsParams): Promise<string> {
    const streamId = `stream_${params.personaId}_${Date.now()}`;
    
    // Create persona stream configuration
    const personaStream: PersonaStream = {
      streamId,
      personaId: params.personaId,
      eventQueue: [],
      streamConfig: params.streamConfig || {},
      eventFilters: params.eventFilters || {},
      responseConfig: params.responseConfig || {},
      status: 'active',
      healthMetrics: {
        connectionUptime: 0,
        reconnectionCount: 0,
        lastHeartbeat: Date.now(),
        eventsReceived: 0,
        eventsProcessed: 0,
        eventsFiltered: 0,
        responsesGenerated: 0,
        averageResponseTime: 0,
        responseSuccessRate: 1.0,
        processingErrors: 0,
        connectionErrors: 0,
        responseErrors: 0
      },
      multiContextEnabled: params.multiContextEnabled || false,
      lastActivity: Date.now()
    };

    this.personaStreams.set(params.personaId, personaStream);
    
    console.log(`📊 SERVER: Created event stream ${streamId} for persona ${params.personaId}`);
    
    return streamId;
  }

  /**
   * Register persona with chat daemon for event routing
   */
  private async registerWithChatDaemon(params: ReceiveEventsParams, streamId: string): Promise<void> {
    try {
      await this.commander.routeCommand({
        command: 'chat/register-event-listener',
        params: {
          listenerId: params.personaId,
          listenerType: 'persona',
          eventTypes: params.eventTypes || [],
          roomIds: params.roomIds,
          streamId,
          routingEndpoint: `/events/persona/${params.personaId}`,
          filters: params.eventFilters
        }
      });

      console.log(`🔗 SERVER: Registered persona ${params.personaId} with chat daemon`);
      
    } catch (error) {
      console.error(`❌ SERVER: Failed to register with chat daemon:`, error);
      throw error;
    }
  }

  /**
   * Initialize multi-context awareness for persona
   */
  private async initializeMultiContext(personaId: string): Promise<void> {
    try {
      // Get persona's current room participations
      const participationResult = await this.commander.routeCommand({
        command: 'chat/get-persona-participations',
        params: { personaId }
      });

      if (participationResult.success) {
        const multiContextState: MultiContextState = {
          personaId,
          lastUpdated: Date.now(),
          activeRooms: new Map(),
          activeDMs: new Map(),
          contextRelationships: [],
          attentionDistribution: {
            primaryFocus: '',
            secondaryFoci: [],
            attentionAllocations: new Map(),
            maxConcurrentContexts: 5
          },
          academyContexts: new Map()
        };

        // Initialize room states
        for (const roomId of participationResult.activeRooms || []) {
          await this.initializeRoomContext(personaId, roomId, multiContextState);
        }

        this.multiContextStates.set(personaId, multiContextState);
        console.log(`🧠 SERVER: Initialized multi-context awareness for ${personaId}`);
      }

    } catch (error) {
      console.warn(`⚠️ SERVER: Multi-context initialization failed:`, error);
    }
  }

  /**
   * Start event processing loop for persona
   */
  private startEventProcessing(personaId: string, streamId: string): void {
    const processEvents = async () => {
      const stream = this.personaStreams.get(personaId);
      if (!stream || stream.status !== 'active') return;

      try {
        // Process queued events
        while (stream.eventQueue.length > 0) {
          const event = stream.eventQueue.shift();
          if (event) {
            await this.processEvent(personaId, event);
          }
        }

        // Update health metrics
        stream.healthMetrics.lastHeartbeat = Date.now();
        
      } catch (error) {
        console.error(`❌ SERVER: Event processing error for ${personaId}:`, error);
        stream.healthMetrics.processingErrors++;
      }

      // Schedule next processing cycle
      setTimeout(processEvents, 1000); // Process every second
    };

    // Start processing
    processEvents();
  }

  /**
   * Process individual event for persona
   */
  private async processEvent(personaId: string, event: ChatEvent): Promise<void> {
    const stream = this.personaStreams.get(personaId);
    if (!stream) return;

    try {
      // Update metrics
      stream.healthMetrics.eventsReceived++;

      // Apply filters
      if (!this.passesFilters(event, stream.eventFilters)) {
        stream.healthMetrics.eventsFiltered++;
        return;
      }

      // Update multi-context state
      if (stream.multiContextEnabled) {
        await this.updateMultiContextState(personaId, event);
      }

      // Determine if response is needed
      const shouldRespond = await this.shouldRespondToEvent(personaId, event, stream);
      
      if (shouldRespond) {
        await this.generateEventResponse(personaId, event, stream);
      }

      stream.healthMetrics.eventsProcessed++;
      stream.lastActivity = Date.now();

    } catch (error) {
      console.error(`❌ SERVER: Error processing event ${event.eventId} for ${personaId}:`, error);
      stream.healthMetrics.processingErrors++;
    }
  }

  /**
   * Check if event passes filters
   */
  private passesFilters(event: ChatEvent, filters: any): boolean {
    // Event type filter
    if (filters.excludedEventTypes?.includes(event.eventType)) {
      return false;
    }

    if (filters.includedEventTypes && !filters.includedEventTypes.includes(event.eventType)) {
      return false;
    }

    // Priority filter
    if (filters.minimumPriority) {
      const priorityLevels = { low: 0, medium: 1, high: 2, urgent: 3 };
      if (priorityLevels[event.priority] < priorityLevels[filters.minimumPriority]) {
        return false;
      }
    }

    // Relevance filter
    if (filters.relevanceThreshold && event.eventContext?.relevance < filters.relevanceThreshold) {
      return false;
    }

    // Keyword filter
    if (filters.keywordFilters && filters.keywordFilters.length > 0) {
      const content = event.eventData.messageContent || '';
      const hasKeyword = filters.keywordFilters.some((keyword: string) => 
        content.toLowerCase().includes(keyword.toLowerCase())
      );
      if (!hasKeyword) {
        return false;
      }
    }

    return true;
  }

  /**
   * Update multi-context state based on event
   */
  private async updateMultiContextState(personaId: string, event: ChatEvent): Promise<void> {
    const multiContext = this.multiContextStates.get(personaId);
    if (!multiContext) return;

    try {
      // Update room context if event is from room
      if (event.roomId) {
        let roomContext = multiContext.activeRooms.get(event.roomId);
        if (!roomContext) {
          roomContext = await this.createRoomContext(event.roomId);
          multiContext.activeRooms.set(event.roomId, roomContext);
        }

        // Update room context with event
        roomContext.lastActivity = event.timestamp;
        roomContext.recentEvents.push(event);
        
        // Keep only recent events
        if (roomContext.recentEvents.length > 50) {
          roomContext.recentEvents = roomContext.recentEvents.slice(-25);
        }
      }

      multiContext.lastUpdated = Date.now();

    } catch (error) {
      console.warn(`⚠️ SERVER: Failed to update multi-context state:`, error);
    }
  }

  /**
   * Determine if persona should respond to event
   */
  private async shouldRespondToEvent(personaId: string, event: ChatEvent, stream: PersonaStream): Promise<boolean> {
    if (!stream.responseConfig.autoResponse) {
      return false;
    }

    // Check response rate limits
    const responseCount = stream.healthMetrics.responsesGenerated;
    const timeWindow = stream.responseConfig.responseTimeWindow || 3600000; // 1 hour
    const maxResponses = stream.responseConfig.maxResponsesPerHour || 60;
    
    if (responseCount >= maxResponses) {
      return false; // Hit rate limit
    }

    // Check if event requires response
    if (event.requiresResponse) {
      return true;
    }

    // Check response criteria
    if (stream.responseConfig.responseCriteria) {
      return await this.evaluateResponseCriteria(personaId, event, stream.responseConfig.responseCriteria);
    }

    return false;
  }

  /**
   * Evaluate response criteria
   */
  private async evaluateResponseCriteria(personaId: string, event: ChatEvent, criteria: any): Promise<boolean> {
    // Relevance check
    if (criteria.minimumRelevance) {
      const relevance = event.eventContext?.relevance || 0;
      if (relevance < criteria.minimumRelevance) {
        return false;
      }
    }

    // Expertise check
    if (criteria.expertiseRequired) {
      const hasExpertise = await this.checkPersonaExpertise(personaId, event);
      if (!hasExpertise) {
        return false;
      }
    }

    // Context appropriateness check
    if (criteria.contextuallyAppropriate) {
      const appropriate = await this.checkContextualAppropriateness(personaId, event);
      if (!appropriate) {
        return false;
      }
    }

    return true;
  }

  /**
   * Generate response to event
   */
  private async generateEventResponse(personaId: string, event: ChatEvent, stream: PersonaStream): Promise<void> {
    try {
      const startTime = Date.now();

      // Build context for response generation
      const responseContext = await this.buildResponseContext(personaId, event);

      // Generate response through AI provider daemon
      const responseResult = await this.commander.routeCommand({
        command: 'ai-provider/generate-event-response',
        params: {
          personaId,
          event,
          context: responseContext,
          responseConfig: stream.responseConfig.responseGeneration
        }
      });

      if (responseResult.success && responseResult.response) {
        // Send response through chat system
        await this.sendEventResponse(personaId, event, responseResult.response);
        
        // Update metrics
        const responseTime = Date.now() - startTime;
        stream.healthMetrics.responsesGenerated++;
        stream.healthMetrics.averageResponseTime = 
          (stream.healthMetrics.averageResponseTime + responseTime) / 2;
        stream.healthMetrics.responseSuccessRate = 
          stream.healthMetrics.responsesGenerated / 
          (stream.healthMetrics.responsesGenerated + stream.healthMetrics.responseErrors);
      }

    } catch (error) {
      console.error(`❌ SERVER: Failed to generate response for ${personaId}:`, error);
      stream.healthMetrics.responseErrors++;
    }
  }

  /**
   * Build context for response generation
   */
  private async buildResponseContext(personaId: string, event: ChatEvent): Promise<any> {
    const context: any = {
      event,
      timestamp: Date.now()
    };

    // Add multi-context awareness
    const multiContext = this.multiContextStates.get(personaId);
    if (multiContext) {
      context.multiRoomContext = {
        activeRooms: Array.from(multiContext.activeRooms.keys()),
        attentionDistribution: multiContext.attentionDistribution,
        contextRelationships: multiContext.contextRelationships
      };
    }

    // Add conversation history
    if (event.eventContext) {
      context.conversationHistory = event.eventContext.conversationHistory;
      context.relationships = event.eventContext.relationships;
    }

    return context;
  }

  /**
   * Send event response
   */
  private async sendEventResponse(personaId: string, event: ChatEvent, response: EventResponse): Promise<void> {
    try {
      await this.commander.routeCommand({
        command: 'chat/send-message',
        params: {
          roomId: event.roomId,
          senderId: personaId,
          content: response.content,
          messageType: 'event_response',
          replyToMessageId: event.eventData.messageId,
          urgency: response.priority,
          capabilitiesUsed: response.capabilitiesUsed,
          learningValue: response.learningValue,
          teachingMoment: response.teachingMoment,
          deliveryOptions: response.deliveryConfig
        }
      });

      console.log(`💬 SERVER: Sent event response from ${personaId} to room ${event.roomId}`);

    } catch (error) {
      console.error(`❌ SERVER: Failed to send event response:`, error);
      throw error;
    }
  }

  /**
   * Setup event handlers for different event types
   */
  private setupEventHandlers(): void {
    // Implementation would set up handlers for routing events from chat daemon
  }

  /**
   * Start health checking for streams
   */
  private startHealthChecking(): void {
    setInterval(() => {
      for (const [personaId, stream] of this.personaStreams) {
        this.checkStreamHealth(personaId, stream);
      }
    }, 30000); // Check every 30 seconds
  }

  /**
   * Start context refreshing
   */
  private startContextRefreshing(): void {
    setInterval(() => {
      for (const [personaId, multiContext] of this.multiContextStates) {
        this.refreshMultiContext(personaId, multiContext);
      }
    }, 10000); // Refresh every 10 seconds
  }

  // Helper methods
  private async initializeRoomContext(personaId: string, roomId: string, multiContext: MultiContextState): Promise<void> {
    // Implementation would initialize room context
  }

  private async createRoomContext(roomId: string): Promise<any> {
    // Implementation would create room context
    return {
      roomId,
      attentionLevel: 0.5,
      lastActivity: Date.now(),
      currentTopic: '',
      recentEvents: [],
      participantStates: new Map()
    };
  }

  private async checkPersonaExpertise(personaId: string, event: ChatEvent): Promise<boolean> {
    // Implementation would check persona expertise
    return true;
  }

  private async checkContextualAppropriateness(personaId: string, event: ChatEvent): Promise<boolean> {
    // Implementation would check appropriateness
    return true;
  }

  private checkStreamHealth(personaId: string, stream: PersonaStream): void {
    // Implementation would check stream health
  }

  private refreshMultiContext(personaId: string, multiContext: MultiContextState): void {
    // Implementation would refresh context
  }

  private generateStreamToken(personaId: string): string {
    return `token_${personaId}_${Date.now()}`;
  }
}

// Supporting interfaces
interface PersonaStream {
  streamId: string;
  personaId: string;
  eventQueue: ChatEvent[];
  streamConfig: any;
  eventFilters: any;
  responseConfig: any;
  status: 'active' | 'paused' | 'error';
  healthMetrics: any;
  multiContextEnabled: boolean;
  lastActivity: number;
}