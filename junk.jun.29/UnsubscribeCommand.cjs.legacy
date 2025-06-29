/**
 * Unsubscribe Command - Remove Event Subscriptions
 */

const BaseCommand = require('../../core/BaseCommand.cjs');

class UnsubscribeCommand extends BaseCommand {
    static metadata = {
        name: 'unsubscribe',
        category: 'events',
        description: 'Unsubscribe from event streams',
        params: {
            subscription_key: {
                type: 'string',
                required: false,
                description: 'Specific subscription to remove'
            },
            event_type: {
                type: 'string',
                required: false,
                description: 'Remove all subscriptions for this event type'
            },
            all: {
                type: 'boolean',
                required: false,
                description: 'Remove all subscriptions for this client'
            }
        },
        examples: [
            'unsubscribe --subscription_key=widget_1:milestone_event',
            'unsubscribe --event_type=browser_event',
            'unsubscribe --all=true'
        ]
    };

    static async execute(params, continuum) {
        const { subscription_key, event_type, all } = params;
        
        if (!continuum.eventSubscriptions) {
            return { success: false, error: 'No active subscriptions' };
        }
        
        const connection = this.getClientConnection(continuum);
        const client_id = connection.id || 'unknown';
        
        let removed = 0;
        
        if (all) {
            // Remove all subscriptions for this client
            for (const [key, subscription] of continuum.eventSubscriptions) {
                if (subscription.connection_id === client_id) {
                    continuum.eventSubscriptions.delete(key);
                    removed++;
                }
            }
        } else if (subscription_key) {
            // Remove specific subscription
            if (continuum.eventSubscriptions.has(subscription_key)) {
                continuum.eventSubscriptions.delete(subscription_key);
                removed = 1;
            }
        } else if (event_type) {
            // Remove all subscriptions for this event type from this client
            for (const [key, subscription] of continuum.eventSubscriptions) {
                if (subscription.event_type === event_type && subscription.connection_id === client_id) {
                    continuum.eventSubscriptions.delete(key);
                    removed++;
                }
            }
        }
        
        console.log(`📡 EVENT UNSUBSCRIBE: ${client_id} removed ${removed} subscriptions`);
        
        return {
            success: true,
            removed_count: removed,
            remaining_subscriptions: continuum.eventSubscriptions.size
        };
    }

    static getClientConnection(continuum) {
        if (continuum && continuum.connection) {
            return continuum.connection;
        }
        if (continuum && continuum.clients && continuum.clients.length > 0) {
            return continuum.clients[0];
        }
        return { id: 'unknown' };
    }
}

module.exports = UnsubscribeCommand;