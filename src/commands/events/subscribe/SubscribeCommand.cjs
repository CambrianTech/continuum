/**
 * Subscribe Command - Event Subscription Management
 * 
 * Allows clients/widgets to subscribe to specific event types:
 * - milestone_event (verification progress)
 * - browser_event (browser launches/status)
 * - command_event (command execution status)
 * - system_event (system health/status)
 */

const BaseCommand = require('../../core/BaseCommand.cjs');

class SubscribeCommand extends BaseCommand {
    static metadata = {
        name: 'subscribe',
        category: 'events',
        description: 'Subscribe to real-time event streams',
        params: {
            event_type: {
                type: 'string',
                required: true,
                description: 'Event type to subscribe to',
                options: ['milestone_event', 'browser_event', 'command_event', 'system_event', 'all']
            },
            client_id: {
                type: 'string',
                required: false,
                description: 'Client identifier for targeted subscriptions'
            },
            filter: {
                type: 'object',
                required: false,
                description: 'Event filter criteria (context, phase, level, etc.)'
            }
        },
        examples: [
            'subscribe milestone_event',
            'subscribe browser_event --filter=\'{"context": "VERIFICATION"}\'',
            'subscribe all --client_id=widget_progress_1'
        ]
    };

    static getExternalResources() {
        return {
            subscriptions: './config/event_subscriptions.json'
        };
    }

    static async execute(params, continuum) {
        const { event_type, client_id, filter } = params;
        
        // Get client connection info
        const connection = this.getClientConnection(continuum);
        const subscriber_id = client_id || connection.id || 'anonymous';
        
        // Initialize event subscription manager if not exists
        if (!continuum.eventSubscriptions) {
            continuum.eventSubscriptions = new Map();
        }
        
        // Create subscription record
        const subscription = {
            subscriber_id,
            event_type,
            filter: filter || {},
            connection_id: connection.id,
            subscribed_at: new Date().toISOString(),
            active: true
        };
        
        // Store subscription
        const subscription_key = `${subscriber_id}:${event_type}`;
        continuum.eventSubscriptions.set(subscription_key, subscription);
        
        // Send confirmation
        this.broadcastToClient(continuum, connection.id, {
            type: 'subscription_confirmed',
            subscription: {
                event_type,
                subscriber_id,
                filter,
                subscription_key
            }
        });
        
        console.log(`📡 EVENT SUBSCRIPTION: ${subscriber_id} → ${event_type}`);
        if (filter && Object.keys(filter).length > 0) {
            console.log(`   🔍 Filter: ${JSON.stringify(filter)}`);
        }
        
        return {
            success: true,
            subscription_key,
            event_type,
            subscriber_id,
            active_subscriptions: continuum.eventSubscriptions.size
        };
    }

    static getClientConnection(continuum) {
        // Get current client connection from WebSocket context
        if (continuum && continuum.connection) {
            return continuum.connection;
        }
        
        // Fallback: use first available client
        if (continuum && continuum.clients && continuum.clients.length > 0) {
            return continuum.clients[0];
        }
        
        return { id: 'unknown' };
    }

    static broadcastToClient(continuum, client_id, message) {
        // Send message to specific client
        if (continuum.wsServer && continuum.wsServer.broadcastToClient) {
            continuum.wsServer.broadcastToClient(client_id, message);
        }
    }
}

module.exports = SubscribeCommand;