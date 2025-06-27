/**
 * Continuum API - TypeScript compiled to ES5 for browser compatibility
 * Single script that provides the continuum object for all interactions
 */

(function() {
    'use strict';
    
    // TypeScript-style interface implementation in JavaScript
    var ContinuumAPI = function() {
        this.ws = null;
        this.connected = false;
        this.messageHandlers = new Map();
        this.eventHandlers = new Map();
    };
    
    ContinuumAPI.prototype.connect = function() {
        var self = this;
        try {
            this.ws = new WebSocket('ws://localhost:9000');
            
            this.ws.onopen = function() {
                self.connected = true;
                console.log('✅ Continuum TypeScript API connected');
                // Dispatch connection event for widgets
                window.dispatchEvent(new CustomEvent('continuum-connected'));
            };
            
            this.ws.onmessage = function(event) {
                try {
                    var data = JSON.parse(event.data);
                    console.log('📨 Command response:', data);
                    
                    // Emit events via clean API
                    if (data.type) {
                        self.emit(data.type, data);
                    }
                    
                    // Handle specific command responses
                    if (data.command && self.messageHandlers.has(data.command)) {
                        self.messageHandlers.get(data.command)(data);
                    }
                } catch (error) {
                    console.error('❌ Message parsing error:', error);
                }
            };
            
            this.ws.onerror = function(error) {
                console.error('❌ WebSocket error:', error);
            };
            
            this.ws.onclose = function() {
                self.connected = false;
                console.log('🔌 Continuum disconnected - attempting reconnect');
                setTimeout(function() { self.connect(); }, 3000);
            };
            
        } catch (error) {
            console.error('❌ Connection failed:', error);
        }
    };
    
    // Main command execution method - everything goes through this
    ContinuumAPI.prototype.execute = function(command, params, callback) {
        params = params || {};
        callback = callback || null;
        
        if (!this.ws || !this.connected) {
            console.warn('⚠️ Continuum not connected - command queued:', command);
            return;
        }
        
        var message = {
            type: 'execute_command',
            command: command,
            params: params,
            timestamp: new Date().toISOString(),
            id: 'cmd_' + Date.now() + '_' + Math.random().toString(36).substr(2, 9)
        };
        
        // Store callback for response
        if (callback) {
            this.messageHandlers.set(message.id, callback);
        }
        
        try {
            this.ws.send(JSON.stringify(message));
            console.log('🎯 Command executed:', command, params);
        } catch (error) {
            console.error('❌ Command send error:', error);
        }
    };
    
    // Convenience methods for common commands
    ContinuumAPI.prototype.chat = function(message, room, callback) {
        return this.execute('chat', { message: message, room: room || 'general' }, callback);
    };
    
    ContinuumAPI.prototype.screenshot = function(params, callback) {
        return this.execute('screenshot', params || {}, callback);
    };
    
    ContinuumAPI.prototype.loadChatMessages = function(room, callback) {
        return this.execute('load_chat_messages', { room: room || 'general' }, callback);
    };
    
    ContinuumAPI.prototype.isConnected = function() {
        return this.connected;
    };
    
    // Event handling - clean API
    ContinuumAPI.prototype.on = function(eventType, handler) {
        if (!this.eventHandlers.has(eventType)) {
            this.eventHandlers.set(eventType, []);
        }
        this.eventHandlers.get(eventType).push(handler);
    };
    
    ContinuumAPI.prototype.off = function(eventType, handler) {
        if (this.eventHandlers.has(eventType)) {
            var handlers = this.eventHandlers.get(eventType);
            var index = handlers.indexOf(handler);
            if (index > -1) {
                handlers.splice(index, 1);
            }
        }
    };
    
    ContinuumAPI.prototype.emit = function(eventType, data) {
        if (this.eventHandlers.has(eventType)) {
            var handlers = this.eventHandlers.get(eventType);
            handlers.forEach(function(handler) {
                try {
                    handler(data);
                } catch (error) {
                    console.error('❌ Event handler error:', error);
                }
            });
        }
    };
    
    // Create and expose global continuum object
    window.continuum = new ContinuumAPI();
    
    // Auto-connect when DOM is ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', function() {
            window.continuum.connect();
        });
    } else {
        window.continuum.connect();
    }
    
    console.log('🚀 Continuum TypeScript API initialized');
    
})();