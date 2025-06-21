/**
 * Event Broadcaster - Core Event Distribution System
 * 
 * Receives events from various sources (Python clients, commands, etc.)
 * and broadcasts them to subscribed clients/widgets
 */

class EventBroadcaster {
    constructor(wsServer) {
        this.wsServer = wsServer;
        this.subscriptions = new Map();
        console.log('📡 Event Broadcaster initialized');
    }

    /**
     * Handle incoming milestone event from Python client
     */
    handleMilestoneEvent(event, source_connection) {
        const { milestone } = event;
        
        console.log(`🎯 MILESTONE EVENT: ${milestone.context}/${milestone.phase} - ${milestone.action}`);
        if (milestone.details) {
            console.log(`   ℹ️  ${milestone.details}`);
        }
        
        // Broadcast to all milestone_event subscribers
        this.broadcastToSubscribers('milestone_event', {
            type: 'milestone_event',
            milestone,
            source: event.source || 'unknown',
            broadcast_time: new Date().toISOString()
        });
    }

    /**
     * Handle browser launch events
     */
    handleBrowserEvent(event, source_connection) {
        console.log(`🌐 BROWSER EVENT: ${event.action}`);
        
        this.broadcastToSubscribers('browser_event', {
            type: 'browser_event',
            ...event,
            broadcast_time: new Date().toISOString()
        });
    }

    /**
     * Handle command execution events
     */
    handleCommandEvent(event, source_connection) {
        console.log(`⚡ COMMAND EVENT: ${event.command} - ${event.status}`);
        
        this.broadcastToSubscribers('command_event', {
            type: 'command_event',
            ...event,
            broadcast_time: new Date().toISOString()
        });
    }

    /**
     * Broadcast event to all subscribers of a specific event type
     */
    broadcastToSubscribers(event_type, event_data) {
        if (!this.subscriptions || this.subscriptions.size === 0) {
            console.log(`📡 No subscribers for ${event_type}`);
            return;
        }

        let sent_count = 0;
        
        for (const [subscription_key, subscription] of this.subscriptions) {
            // Check if subscription matches event type
            if (subscription.event_type === event_type || subscription.event_type === 'all') {
                // Apply filters if specified
                if (this.matchesFilter(event_data, subscription.filter)) {
                    this.sendToClient(subscription.connection_id, event_data);
                    sent_count++;
                }
            }
        }
        
        console.log(`📡 Broadcast ${event_type} to ${sent_count} subscribers`);
    }

    /**
     * Check if event matches subscription filter
     */
    matchesFilter(event_data, filter) {
        if (!filter || Object.keys(filter).length === 0) {
            return true; // No filter = match all
        }
        
        // For milestone events, check context/phase/level filters
        if (event_data.milestone) {
            const { milestone } = event_data;
            
            if (filter.context && milestone.context !== filter.context) {
                return false;
            }
            if (filter.phase && milestone.phase !== filter.phase) {
                return false;
            }
            if (filter.level && milestone.level !== filter.level) {
                return false;
            }
        }
        
        return true;
    }

    /**
     * Send event to specific client
     */
    sendToClient(client_id, event_data) {
        try {
            if (this.wsServer && this.wsServer.broadcastToClient) {
                this.wsServer.broadcastToClient(client_id, event_data);
            } else if (this.wsServer && this.wsServer.broadcast) {
                // Fallback to broadcast to all if targeted send not available
                this.wsServer.broadcast(event_data);
            }
        } catch (error) {
            console.log(`⚠️  Failed to send event to client ${client_id}: ${error.message}`);
        }
    }

    /**
     * Add subscription (called by SubscribeCommand)
     */
    addSubscription(subscription_key, subscription) {
        this.subscriptions.set(subscription_key, subscription);
        console.log(`📡 Added subscription: ${subscription_key}`);
    }

    /**
     * Remove subscription (called by UnsubscribeCommand)
     */
    removeSubscription(subscription_key) {
        const removed = this.subscriptions.delete(subscription_key);
        if (removed) {
            console.log(`📡 Removed subscription: ${subscription_key}`);
        }
        return removed;
    }

    /**
     * Get subscription stats
     */
    getStats() {
        const by_type = {};
        for (const [key, subscription] of this.subscriptions) {
            by_type[subscription.event_type] = (by_type[subscription.event_type] || 0) + 1;
        }
        
        return {
            total_subscriptions: this.subscriptions.size,
            by_event_type: by_type,
            subscription_keys: Array.from(this.subscriptions.keys())
        };
    }
}

module.exports = EventBroadcaster;