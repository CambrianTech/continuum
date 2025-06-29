/**
 * EventBus - Clean, dynamic event system for Continuum
 * Replaces hardcoded message handling with dynamic event publishing
 */

class EventBus {
  constructor(continuum) {
    this.continuum = continuum;
    this.subscribers = new Map(); // eventType:filter -> Set of sessionIds
    this.eventHandlers = new Map(); // messageType -> handler function
    
    // Setup dynamic message handling
    this.setupEventHandlers();
    
    console.log('📡 EventBus: Initialized with dynamic event system');
  }
  
  /**
   * Dynamic event handlers - no hardcoded message types
   */
  setupEventHandlers() {
    // Console log events
    this.registerHandler('client_console_log', (data, sessionId) => {
      this.publishEvent('logs', 'client', {
        type: 'client_log',
        timestamp: new Date(data.timestamp).toISOString(),
        level: data.level,
        message: data.message,
        url: data.url,
        sessionId: sessionId
      });
    });
    
    // Server log events (when server logs are captured)
    this.registerHandler('server_log', (data, sessionId) => {
      this.publishEvent('logs', 'server', {
        type: 'server_log',
        timestamp: new Date().toISOString(),
        level: data.level || 'info',
        message: data.message,
        sessionId: sessionId
      });
    });
    
    // Command execution events
    this.registerHandler('command_execution', (data, sessionId) => {
      this.publishEvent('commands', 'server', {
        type: 'command_execution',
        timestamp: new Date().toISOString(),
        command: data.command,
        params: data.params,
        result: data.result,
        sessionId: sessionId
      });
    });
    
    // Command trace events with originator IDs
    this.registerHandler('command_trace', (data, sessionId) => {
      this.publishEvent('traces', 'server', {
        type: 'command_trace',
        timestamp: new Date().toISOString(),
        originatorId: data.originatorId,
        phase: data.phase,
        command: data.command,
        action: data.action,
        rawParamsType: data.rawParamsType,
        rawParams: data.rawParams,
        parsedParams: data.parsedParams,
        error: data.error,
        sessionId: sessionId
      });
    });
    
    // Connection events
    this.registerHandler('connection_event', (data, sessionId) => {
      this.publishEvent('connections', 'server', {
        type: 'connection',
        timestamp: new Date().toISOString(),
        action: data.action, // 'connect', 'disconnect'
        sessionId: sessionId
      });
    });
    
    // Error events
    this.registerHandler('javascript_error', (data, sessionId) => {
      this.publishEvent('errors', 'client', {
        type: 'javascript_error',
        timestamp: new Date(data.timestamp).toISOString(),
        error: data.error,
        stack: data.stack,
        url: data.url,
        line: data.line,
        column: data.column,
        sessionId: sessionId
      });
    });
  }
  
  /**
   * Register a handler for a message type
   */
  registerHandler(messageType, handler) {
    this.eventHandlers.set(messageType, handler);
    console.log(`📡 EventBus: Registered handler for ${messageType}`);
  }
  
  /**
   * Process incoming WebSocket message and trigger events
   */
  processMessage(messageType, data, sessionId) {
    const handler = this.eventHandlers.get(messageType);
    if (handler) {
      try {
        handler(data, sessionId);
      } catch (error) {
        console.error(`📡 EventBus: Handler failed for ${messageType}:`, error);
      }
    }
  }
  
  /**
   * Subscribe to events
   */
  subscribe(eventType, filter, sessionId) {
    const subscriptionKey = `${eventType}:${filter}`;
    
    if (!this.subscribers.has(subscriptionKey)) {
      this.subscribers.set(subscriptionKey, new Set());
    }
    
    this.subscribers.get(subscriptionKey).add(sessionId);
    
    console.log(`📡 EventBus: Session ${sessionId} subscribed to ${subscriptionKey}`);
    return {
      eventType,
      filter,
      sessionId,
      totalSubscribers: this.subscribers.get(subscriptionKey).size
    };
  }
  
  /**
   * Unsubscribe from events
   */
  unsubscribe(eventType, filter, sessionId) {
    const subscriptionKey = `${eventType}:${filter}`;
    
    if (this.subscribers.has(subscriptionKey)) {
      this.subscribers.get(subscriptionKey).delete(sessionId);
      
      // Clean up empty subscriptions
      if (this.subscribers.get(subscriptionKey).size === 0) {
        this.subscribers.delete(subscriptionKey);
      }
    }
    
    console.log(`📡 EventBus: Session ${sessionId} unsubscribed from ${subscriptionKey}`);
    return {
      eventType,
      filter,
      sessionId,
      remainingSubscribers: this.subscribers.get(subscriptionKey)?.size || 0
    };
  }
  
  /**
   * List active subscriptions
   */
  listSubscriptions(sessionId = null) {
    const subscriptions = [];
    
    for (const [key, sessions] of this.subscribers.entries()) {
      const [eventType, filter] = key.split(':');
      subscriptions.push({
        eventType,
        filter,
        subscriberCount: sessions.size,
        isSubscribed: sessionId ? sessions.has(sessionId) : false
      });
    }
    
    return {
      subscriptions,
      totalSubscriptions: subscriptions.length
    };
  }
  
  /**
   * Publish event to all matching subscribers
   */
  publishEvent(eventType, source, eventData) {
    // Find matching subscriptions
    const subscriptionKeys = [
      `${eventType}:${source}`,
      `${eventType}:both`,
      `all:${source}`,
      `all:both`
    ];
    
    let delivered = 0;
    
    for (const key of subscriptionKeys) {
      if (this.subscribers.has(key)) {
        const subscribers = this.subscribers.get(key);
        
        // Send event to all subscribers
        for (const sessionId of subscribers) {
          if (this.sendEventToSession(sessionId, {
            type: 'event_stream',
            eventType,
            source,
            data: eventData,
            timestamp: new Date().toISOString()
          })) {
            delivered++;
          }
        }
      }
    }
    
    if (delivered > 0) {
      console.log(`📡 EventBus: Published ${eventType}:${source} to ${delivered} subscribers`);
    } else if (this.subscribers.size > 0) {
      console.log(`📡 EventBus: No subscribers for ${eventType}:${source} (have ${this.subscribers.size} total subscriptions)`);
    }
  }
  
  /**
   * Send event to specific session via WebSocket
   */
  sendEventToSession(sessionId, eventMessage) {
    // Use Continuum's activeConnections Map
    if (this.continuum.activeConnections && this.continuum.activeConnections.has(sessionId)) {
      const ws = this.continuum.activeConnections.get(sessionId);
      
      if (ws.readyState === 1) { // WebSocket.OPEN
        try {
          ws.send(JSON.stringify(eventMessage));
          return true;
        } catch (error) {
          console.error(`📡 EventBus: Failed to send event to session ${sessionId}:`, error);
          return false;
        }
      }
    }
    return false;
  }
  
  /**
   * Get subscription stats
   */
  getStats() {
    const stats = {
      totalSubscriptions: this.subscribers.size,
      totalSubscribers: 0,
      eventTypes: {}
    };
    
    for (const [key, sessions] of this.subscribers.entries()) {
      const [eventType] = key.split(':');
      stats.totalSubscribers += sessions.size;
      
      if (!stats.eventTypes[eventType]) {
        stats.eventTypes[eventType] = 0;
      }
      stats.eventTypes[eventType] += sessions.size;
    }
    
    return stats;
  }
}

module.exports = EventBus;