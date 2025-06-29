/**
 * Dynamic Message Router - Routes WebSocket messages to daemon capabilities dynamically
 * No hardcoded handlers - discovers and routes based on daemon capabilities
 */

import { EventEmitter } from 'events';
import { WebSocketMessage } from '../types';
import { DaemonConnector } from './DaemonConnector';

export interface RegisteredDaemon {
  name: string;
  daemon: any; // BaseDaemon instance
  capabilities: string[];
  messageTypes: string[];
}

export class DynamicMessageRouter extends EventEmitter {
  private registeredDaemons = new Map<string, RegisteredDaemon>();
  private routerVersion: string = '1.0.0';

  constructor() {
    super();
    // Load version asynchronously, don't block constructor
    this.loadCurrentVersionSync().catch(error => {
      console.warn(`⚠️ Failed to load version in constructor: ${error.message}`);
    });
  }

  private async loadCurrentVersionSync(): Promise<void> {
    try {
      // Use ES module imports
      const { readFileSync } = await import('fs');
      const packageData = JSON.parse(readFileSync('./package.json', 'utf8'));
      this.routerVersion = packageData.version;
      console.log(`📦 DynamicMessageRouter loaded version: ${this.routerVersion}`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.warn(`⚠️ DynamicMessageRouter failed to load version: ${errorMessage}`);
      this.routerVersion = '0.2.UNKNOWN';
    }
  }

  /**
   * Register a daemon and discover its capabilities automatically
   */
  async registerDaemon(name: string, daemon: any): Promise<void> {
    console.log(`🔌 Registering daemon: ${name}`);
    
    try {
      // Get daemon capabilities
      const capResponse = await daemon.handleMessage({ type: 'get_capabilities', data: {} });
      const capabilities = capResponse.success ? (capResponse.data.capabilities || []) : [];
      
      // Discover supported message types by introspection
      const messageTypes = this.discoverMessageTypes(daemon);
      
      this.registeredDaemons.set(name, {
        name,
        daemon,
        capabilities,
        messageTypes
      });
      
      console.log(`✅ Registered ${name} with capabilities: ${capabilities.join(', ')}`);
      console.log(`📝 Message types: ${messageTypes.join(', ')}`);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.warn(`⚠️ Failed to register daemon ${name}:`, errorMessage);
    }
  }

  /**
   * Route message to appropriate daemon based on message type
   */
  async routeMessage(
    message: any,
    clientId: string,
    _daemonConnector?: DaemonConnector
  ): Promise<WebSocketMessage | null> {
    const messageType = message.type;
    
    // Find daemon that can handle this message type
    const targetDaemon = this.findHandlerDaemon(messageType);
    
    if (!targetDaemon) {
      console.log(`🔄 No daemon found for message type: ${messageType}`);
      return {
        type: 'error',
        data: { 
          error: `No daemon registered for message type: ${messageType}`,
          availableTypes: this.getAllMessageTypes(),
          routerVersion: this.routerVersion,
          component: 'DynamicMessageRouter'
        },
        timestamp: new Date().toISOString(),
        clientId
      };
    }

    try {
      console.log(`📨 Routing ${messageType} to daemon: ${targetDaemon.name}`);
      
      const result = await targetDaemon.daemon.handleMessage({
        type: messageType,
        data: message.data
      });
      
      return {
        type: `${messageType}_response`,
        data: result.data,
        timestamp: new Date().toISOString(),
        clientId,
        requestId: message.requestId,
        processedBy: targetDaemon.name
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Handler error for ${messageType} in ${targetDaemon.name}:`, errorMessage);
      return {
        type: 'error',
        data: { 
          error: errorMessage,
          daemon: targetDaemon.name,
          messageType,
          routerVersion: this.routerVersion,
          component: 'DynamicMessageRouter'
        },
        timestamp: new Date().toISOString(),
        clientId,
        requestId: message.requestId
      };
    }
  }

  /**
   * Find which daemon can handle a specific message type (prefer specific capabilities)
   */
  private findHandlerDaemon(messageType: string): RegisteredDaemon | null {
    // First, look for daemons with specific capabilities for this message
    const capabilityMap = {
      'get_capabilities': ['basic-rendering', 'legacy-ui', 'modern-ui'],
      'render_request': ['basic-rendering', 'legacy-ui', 'modern-ui'],
      'switch_engine': ['basic-rendering', 'legacy-ui', 'modern-ui'],
      'tabRegister': ['websocket-server', 'client-management'],
      'get_component_css': ['websocket-server', 'css-service']
    };
    
    const preferredCapabilities = capabilityMap[messageType as keyof typeof capabilityMap];
    if (preferredCapabilities) {
      for (const daemon of this.registeredDaemons.values()) {
        const hasCapability = daemon.capabilities.some(cap => 
          preferredCapabilities.includes(cap)
        );
        if (hasCapability && daemon.messageTypes.includes(messageType)) {
          console.log(`🎯 Found specialized daemon ${daemon.name} for ${messageType}`);
          return daemon;
        }
      }
    }
    
    // Fallback: find any daemon that supports the message type
    for (const daemon of this.registeredDaemons.values()) {
      if (daemon.messageTypes.includes(messageType)) {
        console.log(`🔄 Using fallback daemon ${daemon.name} for ${messageType}`);
        return daemon;
      }
    }
    
    return null;
  }

  /**
   * Discover what message types a daemon supports by introspection
   */
  private discoverMessageTypes(daemon: any): string[] {
    const messageTypes: string[] = [];
    
    // FIRST: Check if daemon has getMessageTypes() method (CRITICAL FIX)
    if (typeof daemon.getMessageTypes === 'function') {
      try {
        const daemonSpecificTypes = daemon.getMessageTypes();
        if (Array.isArray(daemonSpecificTypes)) {
          messageTypes.push(...daemonSpecificTypes);
          console.log(`✅ Discovered daemon-specific message types: ${daemonSpecificTypes.join(', ')}`);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.warn(`⚠️ Failed to get daemon-specific message types:`, errorMessage);
      }
    }
    
    // SECOND: Add common daemon message types
    const commonTypes = [
      'ping', 'pong',  // Essential for chat communication
      'get_stats', 'get_clients', 'get_capabilities',
      'render_request', 'switch_engine', 
      'execute_command', 'broadcast_message', 'send_message',
      'tabRegister', 'get_component_css'
    ];
    
    // Add common types if daemon has handleMessage
    if (typeof daemon.handleMessage === 'function') {
      for (const type of commonTypes) {
        if (!messageTypes.includes(type)) {
          messageTypes.push(type);
        }
      }
    }
    
    console.log(`📝 Total discovered message types for daemon: ${messageTypes.join(', ')}`);
    return messageTypes;
  }

  /**
   * Get all supported message types across all daemons
   */
  getAllMessageTypes(): string[] {
    const types = new Set<string>();
    for (const daemon of this.registeredDaemons.values()) {
      daemon.messageTypes.forEach(type => types.add(type));
    }
    return Array.from(types);
  }

  /**
   * Get system status showing all registered daemons
   */
  getSystemStatus(): any {
    return {
      registeredDaemons: this.registeredDaemons.size,
      daemons: Array.from(this.registeredDaemons.values()).map(d => ({
        name: d.name,
        capabilities: d.capabilities,
        messageTypes: d.messageTypes
      })),
      totalMessageTypes: this.getAllMessageTypes().length
    };
  }
}