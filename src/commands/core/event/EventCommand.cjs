/**
 * Event Command - Real-time event subscription system
 * Handles subscribe/unsubscribe for live event streams
 */

const BaseCommand = require('../../BaseCommand.cjs');

class EventCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'event',
      description: 'Real-time event subscription system',
      icon: '📡',
      parameters: {
        action: {
          type: 'string',
          required: false,
          description: 'Action: subscribe, unsubscribe, list',
          default: 'list'
        },
        event: {
          type: 'string',
          required: false,
          description: 'Event type: logs, errors, commands, connections, all',
          default: 'logs'
        },
        filter: {
          type: 'string',
          required: false,
          description: 'Event filter: client, server, both, or specific pattern',
          default: 'both'
        },
        sessionId: {
          type: 'string',
          required: false,
          description: 'Client session ID for subscription management'
        }
      },
      examples: [
        'event',
        'event --action subscribe --event logs --filter client',
        'event --action unsubscribe --event logs',
        'event --action list'
      ]
    };
  }

  static async execute(params, continuum) {
    console.log(`📡 EVENT: Starting with params: ${params}`);
    
    const options = this.parseParams(params);
    const action = options.action || 'list';
    const eventType = options.event || 'logs';
    const filter = options.filter || 'both';
    const sessionId = options.sessionId;
    
    console.log(`📡 EVENT: Action: ${action}, Event: ${eventType}, Filter: ${filter}`);
    
    try {
      if (action === 'subscribe') {
        return await this.subscribeToEvent(continuum, eventType, filter, sessionId);
      } else if (action === 'unsubscribe') {
        return await this.unsubscribeFromEvent(continuum, eventType, filter, sessionId);
      } else if (action === 'list') {
        return await this.listSubscriptions(continuum, sessionId);
      } else {
        return this.createErrorResult('Invalid action', `Unknown action: ${action}`);
      }
    } catch (error) {
      console.error('❌ Event command failed:', error);
      return this.createErrorResult('Event command failed', error.message);
    }
  }
  
  static async subscribeToEvent(continuum, eventType, filter, sessionId) {
    // Initialize EventBus if not exists
    if (!continuum.eventBus) {
      const EventBus = require('../../../integrations/EventBus.cjs');
      continuum.eventBus = new EventBus(continuum);
      console.log('📡 EVENT: Initialized EventBus');
    }
    
    // Subscribe via EventBus
    const result = continuum.eventBus.subscribe(eventType, filter, sessionId);
    
    return this.createSuccessResult(result, `Subscribed to ${eventType} events with filter: ${filter}`);
  }
  
  static async unsubscribeFromEvent(continuum, eventType, filter, sessionId) {
    if (!continuum.eventBus) {
      return this.createSuccessResult({}, 'No active subscriptions');
    }
    
    // Unsubscribe via EventBus
    const result = continuum.eventBus.unsubscribe(eventType, filter, sessionId);
    
    return this.createSuccessResult(result, `Unsubscribed from ${eventType} events`);
  }
  
  static async listSubscriptions(continuum, sessionId) {
    if (!continuum.eventBus) {
      return this.createSuccessResult({ subscriptions: [] }, 'No active subscriptions');
    }
    
    // List subscriptions via EventBus
    const result = continuum.eventBus.listSubscriptions(sessionId);
    
    return this.createSuccessResult(result, `Found ${result.totalSubscriptions} active event subscriptions`);
  }
  
  // EventBus handles all event publishing dynamically - no hardcoded setup needed
}

module.exports = EventCommand;