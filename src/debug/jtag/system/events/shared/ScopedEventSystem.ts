/**
 * Scoped Event System - Advanced Event Subscription Architecture
 * 
 * BREAKTHROUGH: Provides room-scoped, user-scoped, and system-scoped events
 * that automatically filter subscriptions. Widgets only receive events for 
 * rooms they're actually in, preventing event spam and irrelevant updates.
 * 
 * Key Features:
 * - jtag.events.room('room-123').on('chat:message-received', handler)
 * - jtag.events.user('user-456').on('session:status-update', handler)  
 * - jtag.events.system.on('system:ready', handler)
 * - Automatic subscription cleanup
 * - Cross-context event bridging (browser ↔ server)
 * - Type-safe event constants (no magic strings)
 */

import type { EventsInterface } from './JTAGEventSystem';
import { EventManager } from './JTAGEventSystem';
import type { JTAGRouter } from '../../core/router/shared/JTAGRouter';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../core/types/CrossPlatformUUID';
import { JTAGMessageFactory } from '../../core/types/JTAGTypes';

/**
 * Scope configuration for event subscriptions
 */
export interface EventScope {
  type: 'system' | 'room' | 'user' | 'global';
  id?: string; // room ID or user ID for scoped events
  sessionId?: string; // subscriber session ID for tracking
}

/**
 * Enhanced event subscription with metadata
 */
export interface ScopedEventSubscription {
  id: UUID;
  eventName: string;
  scope: EventScope;
  listener: (data?: any) => void;
  createdAt: Date;
  lastTriggered?: Date;
  triggerCount: number;
}

/**
 * System-scoped event interface (extends base with type safety)
 */
export interface SystemScopedEvents extends EventsInterface {
  // All standard event methods but restricted to system events
}

/**
 * Room-scoped event interface
 */
export interface RoomScopedEvents extends EventsInterface {
  // Standard event methods but scoped to specific room
  getSubscriberCount(): number;
  getRoomId(): string;
}

/**
 * User-scoped event interface
 */
export interface UserScopedEvents extends EventsInterface {
  // Standard event methods but scoped to specific user
  getSubscriberCount(): number;
  getUserId(): string;
}

/**
 * Scoped Events Interface - Main interface for widgets
 */
export interface ScopedEventsInterface {
  // System-level events (daemon lifecycle, health, etc.)
  system: SystemScopedEvents;
  
  // Room-scoped events (chat messages, participant changes)
  room(roomId: string): RoomScopedEvents;
  
  // User-scoped events (session changes, personal state)
  user(userId: string): UserScopedEvents;
  
  // Global events (backwards compatibility)
  global: EventsInterface;
}

/**
 * Scoped Event System - Advanced event subscription management
 */
export class ScopedEventSystem {
  private readonly eventManager = new EventManager();
  private readonly subscriptions = new Map<UUID, ScopedEventSubscription>();
  private readonly scopeSubscriptions = new Map<string, Set<UUID>>(); // scope-key -> subscription IDs
  private readonly crossContextBridge: EventBridge;
  
  constructor(
    private readonly router: JTAGRouter,
    private readonly sessionId: UUID
  ) {
    this.crossContextBridge = new EventBridge(router, sessionId);
  }

  /**
   * Main scoped events interface for widgets
   */
  get scopedEvents(): ScopedEventsInterface {
    return {
      system: this.createSystemScopedEvents(),
      room: (roomId: string) => this.createRoomScopedEvents(roomId),
      user: (userId: string) => this.createUserScopedEvents(userId),
      global: this.eventManager.events
    };
  }

  /**
   * Create system-scoped events interface
   */
  private createSystemScopedEvents(): SystemScopedEvents {
    const scope: EventScope = { type: 'system', sessionId: this.sessionId };
    return this.createScopedEventsInterface(scope) as SystemScopedEvents;
  }

  /**
   * Create room-scoped events interface
   */
  private createRoomScopedEvents(roomId: string): RoomScopedEvents {
    const scope: EventScope = { type: 'room', id: roomId, sessionId: this.sessionId };
    const baseInterface = this.createScopedEventsInterface(scope);
    
    return {
      ...baseInterface,
      getSubscriberCount: () => this.getScopeSubscriberCount(this.getScopeKey(scope)),
      getRoomId: () => roomId
    } as RoomScopedEvents;
  }

  /**
   * Create user-scoped events interface
   */
  private createUserScopedEvents(userId: string): UserScopedEvents {
    const scope: EventScope = { type: 'user', id: userId, sessionId: this.sessionId };
    const baseInterface = this.createScopedEventsInterface(scope);
    
    return {
      ...baseInterface,
      getSubscriberCount: () => this.getScopeSubscriberCount(this.getScopeKey(scope)),
      getUserId: () => userId
    } as UserScopedEvents;
  }

  /**
   * Create scoped events interface for any scope
   */
  private createScopedEventsInterface(scope: EventScope): EventsInterface {
    return {
      emit: (eventName: string, data?: any) => {
        this.emitScopedEvent(scope, eventName, data);
      },
      
      on: (eventName: string, listener: (data?: any) => void): (() => void) => {
        return this.subscribeScopedEvent(scope, eventName, listener);
      },
      
      waitFor: async (eventName: string, timeout = 5000): Promise<any> => {
        return this.waitForScopedEvent(scope, eventName, timeout);
      }
    };
  }

  /**
   * Subscribe to scoped event with automatic filtering
   */
  private subscribeScopedEvent(
    scope: EventScope,
    eventName: string,
    listener: (data?: any) => void
  ): () => void {
    const subscription: ScopedEventSubscription = {
      id: generateUUID(),
      eventName,
      scope,
      listener,
      createdAt: new Date(),
      triggerCount: 0
    };
    
    // Store subscription
    this.subscriptions.set(subscription.id, subscription);
    
    // Add to scope index
    const scopeKey = this.getScopeKey(scope);
    if (!this.scopeSubscriptions.has(scopeKey)) {
      this.scopeSubscriptions.set(scopeKey, new Set());
    }
    this.scopeSubscriptions.get(scopeKey)!.add(subscription.id);
    
    // Create wrapped listener that checks scope before triggering
    const wrappedListener = (data?: any) => {
      if (this.shouldReceiveEvent(scope, eventName, data)) {
        subscription.lastTriggered = new Date();
        subscription.triggerCount++;
        listener(data);
      }
    };
    
    // Subscribe to underlying event system
    const unsubscribe = this.eventManager.events.on(eventName, wrappedListener);
    
    console.log(`🔗 Scoped subscription: ${scope.type}${scope.id ? `:${scope.id}` : ''} → ${eventName}`);
    
    // Return unsubscribe function that cleans up everything
    return () => {
      unsubscribe();
      this.subscriptions.delete(subscription.id);
      this.scopeSubscriptions.get(scopeKey)?.delete(subscription.id);
      console.log(`🔌 Scoped unsubscription: ${scope.type}${scope.id ? `:${scope.id}` : ''} ← ${eventName}`);
    };
  }

  /**
   * Emit scoped event with automatic targeting
   */
  private emitScopedEvent(scope: EventScope, eventName: string, data?: any): void {
    // Enhance data with scope information for filtering
    const scopedData = {
      ...data,
      _scope: scope,
      _emittedBy: this.sessionId,
      _timestamp: new Date().toISOString()
    };
    
    // Emit to underlying event system (subscribers will filter)
    this.eventManager.events.emit(eventName, scopedData);
    
    // Also bridge to other contexts if needed
    this.crossContextBridge.bridgeEvent(scope, eventName, scopedData);
  }

  /**
   * Wait for scoped event with timeout
   */
  private async waitForScopedEvent(
    scope: EventScope,
    eventName: string,
    timeout: number
  ): Promise<any> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error(`Timeout waiting for scoped event: ${scope.type}:${eventName}`));
      }, timeout);
      
      const unsubscribe = this.subscribeScopedEvent(scope, eventName, (data) => {
        clearTimeout(timer);
        unsubscribe();
        resolve(data);
      });
    });
  }

  /**
   * Check if subscription should receive this event based on scope
   */
  private shouldReceiveEvent(scope: EventScope, eventName: string, data?: any): boolean {
    // Global scope always receives events
    if (scope.type === 'global') {
      return true;
    }
    
    // System scope receives system events
    if (scope.type === 'system') {
      return eventName.startsWith('system.') || eventName.includes('daemon') || eventName.includes('health');
    }
    
    // Room scope - check if data targets this room
    if (scope.type === 'room' && scope.id) {
      return (
        data?._scope?.type === 'room' && data?._scope?.id === scope.id ||
        data?.roomId === scope.id ||
        eventName.includes('room') && data?.targetRoomId === scope.id
      );
    }
    
    // User scope - check if data targets this user
    if (scope.type === 'user' && scope.id) {
      return (
        data?._scope?.type === 'user' && data?._scope?.id === scope.id ||
        data?.userId === scope.id ||
        data?.sessionId === scope.id ||
        eventName.includes('session') && data?.targetSessionId === scope.id
      );
    }
    
    return false;
  }

  /**
   * Generate scope key for indexing
   */
  private getScopeKey(scope: EventScope): string {
    return scope.id ? `${scope.type}:${scope.id}` : scope.type;
  }

  /**
   * Get subscriber count for scope
   */
  private getScopeSubscriberCount(scopeKey: string): number {
    return this.scopeSubscriptions.get(scopeKey)?.size || 0;
  }

  /**
   * Get subscription statistics
   */
  getSubscriptionStats() {
    const stats = {
      totalSubscriptions: this.subscriptions.size,
      scopes: {} as Record<string, number>,
      events: {} as Record<string, number>
    };
    
    for (const subscription of this.subscriptions.values()) {
      const scopeKey = this.getScopeKey(subscription.scope);
      stats.scopes[scopeKey] = (stats.scopes[scopeKey] || 0) + 1;
      stats.events[subscription.eventName] = (stats.events[subscription.eventName] || 0) + 1;
    }
    
    return stats;
  }

  /**
   * Clean up disconnected subscriptions
   */
  cleanupDisconnectedSubscriptions(activeSessionIds: Set<string>): void {
    for (const [id, subscription] of this.subscriptions.entries()) {
      if (subscription.scope.sessionId && !activeSessionIds.has(subscription.scope.sessionId)) {
        console.log(`🧹 Cleaning up disconnected subscription: ${subscription.eventName} for session ${subscription.scope.sessionId}`);
        
        // Remove from all tracking structures
        this.subscriptions.delete(id);
        const scopeKey = this.getScopeKey(subscription.scope);
        this.scopeSubscriptions.get(scopeKey)?.delete(id);
      }
    }
  }
}

/**
 * Cross-Context Event Bridge - Routes events between browser and server
 */
export class EventBridge {
  constructor(
    private readonly router: JTAGRouter,
    private readonly sessionId: UUID
  ) {}

  /**
   * Bridge event to other contexts if needed
   */
  async bridgeEvent(scope: EventScope, eventName: string, data: any): Promise<void> {
    // Only bridge certain event types that need cross-context propagation
    if (this.shouldBridgeEvent(scope, eventName)) {
      try {
        // Create bridge message that other contexts can receive
        const bridgeMessage = {
          type: 'event-bridge',
          scope,
          eventName,
          data,
          originSessionId: this.sessionId,
          timestamp: new Date().toISOString(),
          // Required JTAGPayload fields
          context: { environment: 'server' as const, uuid: generateUUID() },
          sessionId: this.sessionId
        };
        
        // Route via existing transport system using proper message factory
        const eventMessage = JTAGMessageFactory.createEvent(
          { environment: 'server', uuid: generateUUID() },
          'EventsDaemon',
          'event-bridge',
          bridgeMessage
        );
        
        await this.router.postMessage(eventMessage);
        
      } catch (error) {
        console.warn(`⚠️ Event bridge failed for ${eventName}:`, error);
      }
    }
  }

  /**
   * Determine if event should be bridged to other contexts
   */
  private shouldBridgeEvent(scope: EventScope, eventName: string): boolean {
    // System events should be bridged
    if (scope.type === 'system') {
      return true;
    }
    
    // Room events for chat should be bridged
    if (scope.type === 'room' && eventName.includes('chat')) {
      return true;
    }
    
    // Session/user events should be bridged
    if (scope.type === 'user' && eventName.includes('session')) {
      return true;
    }
    
    return false;
  }
}