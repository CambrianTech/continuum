// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Universal Identity Foundation - Abstract base for all identity types
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Abstract foundation methods and capability system
 * - Integration tests: Specialized class implementations
 * - Interface compliance: Contract enforcement across all identity types
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Abstract foundation with 90% shared logic, 10% specialized overrides
 * - Universal capability system - eliminates redundant capability definitions
 * - Modular design - prevents god objects, promotes specialized classes
 * - Separation of burden - foundation handles infrastructure, classes handle specifics
 * 
 * REFACTORED FROM GOD OBJECT TO PROPER FOUNDATION
 * 
 * Previous CondensedIdentity was 738 lines - becoming a god object
 * This splits into:
 * - UniversalIdentity (foundation ~200 lines)
 * - HumanUser, PersonaAgent, AIAssistant, SystemAgent (specialized classes)
 * 
 * Principles:
 * - Foundation provides infrastructure (events, capabilities, state)
 * - Specialized classes provide specific implementations
 * - Each class has single responsibility
 * - Easy to extend with new identity types
 */

import { generateUUID } from '../../academy/shared/AcademyTypes';

// ==================== UNIVERSAL TYPES ====================

/**
 * Base capabilities - only truly universal capabilities
 */
export interface BaseCapabilities {
  // Core capabilities only
  communicate: boolean;
  serialize: boolean;
  
  // Extensible for subclasses
  [key: string]: boolean;
}

/**
 * Base metadata - only truly universal properties
 */
export interface BaseMetadata {
  // Core metadata only
  version?: string;
  source?: string;
  description?: string;
  
  // Activity metadata
  lastActivity?: number;
  isActive?: boolean;
  
  // Extensible for subclasses
  [key: string]: any;
}

/**
 * Universal state - consolidates all state tracking
 */
export interface UniversalState {
  status: 'initializing' | 'active' | 'idle' | 'error' | 'destroyed';
  error?: string;
  
  // Context-specific state
  context: 'chat' | 'academy' | 'hybrid' | 'system';
  
  // Performance state
  responseTime?: number;
  successRate?: number;
  
  // Extensible state data
  data?: any;
}

/**
 * Universal event - consolidates all event types
 */
export interface UniversalEvent {
  id: string;
  type: string;
  timestamp: number;
  success: boolean;
  details: any;
  
  // Context
  context: 'chat' | 'academy' | 'hybrid' | 'system';
  
  // Optional associations
  roomId?: string;
  challengeId?: string;
  targetId?: string;
  
  // Learning-specific properties
  learningGain?: number;
}

// ==================== ABSTRACT FOUNDATION ====================

/**
 * Universal Identity Foundation - Abstract base for all identity types
 * 
 * This abstract class provides the 90% shared infrastructure:
 * - Universal capability system
 * - Event recording and retrieval
 * - State management
 * - Serialization/deserialization
 * - Lifecycle management
 * 
 * Specialized classes provide the 10% specific implementations:
 * - Message handling
 * - Initialization logic
 * - Communication patterns
 * - Learning behaviors
 * 
 * Generic types follow inheritance naturally - no complex type manipulations needed
 */
export abstract class UniversalIdentity<
  TMetadata extends BaseMetadata = BaseMetadata,
  TCapabilities extends BaseCapabilities = BaseCapabilities
> {
  // Core properties
  public readonly id: string;
  public name: string;
  public type: string;
  public created: number;
  public metadata: TMetadata;
  
  // Universal systems
  private capabilities: TCapabilities;
  private state: UniversalState;
  private events: UniversalEvent[] = [];
  private eventHandlers: Map<string, Function[]> = new Map();
  
  constructor(config: {
    id?: string;
    name: string;
    type: string;
    metadata: TMetadata;
    capabilities: TCapabilities;
  }) {
    this.id = config.id || generateUUID();
    this.name = config.name;
    this.type = config.type;
    this.created = Date.now();
    this.metadata = config.metadata;
    this.capabilities = config.capabilities;
    
    // Initialize state
    this.state = {
      status: 'initializing',
      context: this.inferContext(config.type)
    };
    
    this.logMessage(`🎯 ${this.type} identity created: ${this.name}`);
  }
  
  // ==================== ABSTRACT METHODS (MUST BE IMPLEMENTED) ====================
  
  /**
   * Handle incoming message - specialized per identity type
   */
  abstract handleMessage(message: any): Promise<any>;
  
  /**
   * Initialize type-specific functionality
   */
  abstract initializeSpecific(): Promise<void>;
  
  /**
   * Cleanup type-specific resources
   */
  abstract destroySpecific(): Promise<void>;
  
  // ==================== UNIVERSAL INFRASTRUCTURE (90% SHARED) ====================
  
  /**
   * Initialize the identity
   */
  async initialize(): Promise<void> {
    this.state.status = 'initializing';
    this.logMessage('🚀 Initializing identity');
    
    try {
      // Initialize type-specific functionality
      await this.initializeSpecific();
      
      this.state.status = 'active';
      this.recordEvent('identity_initialized', { success: true });
      this.logMessage('✅ Identity initialized successfully');
      
    } catch (error) {
      this.state.status = 'error';
      this.state.error = error instanceof Error ? error.message : String(error);
      this.recordEvent('identity_initialization_failed', { success: false, error: this.state.error });
      this.logMessage('❌ Identity initialization failed', error);
      throw error;
    }
  }
  
  /**
   * Destroy the identity
   */
  async destroy(): Promise<void> {
    this.logMessage('🛑 Destroying identity');
    
    try {
      // Cleanup type-specific resources
      await this.destroySpecific();
      
      this.state.status = 'destroyed';
      this.recordEvent('identity_destroyed', { success: true });
      this.logMessage('✅ Identity destroyed successfully');
      
    } catch (error) {
      this.state.status = 'error';
      this.state.error = error instanceof Error ? error.message : String(error);
      this.recordEvent('identity_destruction_failed', { success: false, error: this.state.error });
      this.logMessage('❌ Identity destruction failed', error);
      throw error;
    }
  }
  
  /**
   * Get current capabilities - type follows inheritance
   */
  getCapabilities(): TCapabilities {
    return { ...this.capabilities };
  }
  
  /**
   * Update capabilities - type follows inheritance
   */
  updateCapabilities(updates: Partial<TCapabilities>): void {
    const oldCapabilities = { ...this.capabilities };
    this.capabilities = { ...this.capabilities, ...updates };
    
    this.recordEvent('capabilities_updated', {
      success: true,
      old: oldCapabilities,
      new: this.capabilities
    });
    
    this.logMessage('🔧 Capabilities updated', updates);
  }
  
  /**
   * Check if identity has specific capability
   */
  hasCapability(capability: string): boolean {
    return this.capabilities[capability] === true;
  }
  
  /**
   * Get current state
   */
  getState(): UniversalState {
    return { ...this.state };
  }
  
  /**
   * Update state
   */
  updateState(updates: Partial<UniversalState>): void {
    const oldState = { ...this.state };
    this.state = { ...this.state, ...updates };
    
    this.recordEvent('state_updated', {
      success: true,
      old: oldState,
      new: this.state
    });
  }
  
  /**
   * Record an event
   */
  recordEvent(type: string, details: any): void {
    const event: UniversalEvent = {
      id: generateUUID(),
      type,
      timestamp: Date.now(),
      success: details.success !== false,
      details,
      context: this.state.context,
      ...details
    };
    
    this.events.push(event);
    
    // Limit event history
    if (this.events.length > 1000) {
      this.events = this.events.slice(-1000);
    }
    
    // Emit event to handlers
    this.emit(type, event);
  }
  
  /**
   * Get event history
   */
  getEvents(limit?: number): UniversalEvent[] {
    const events = [...this.events];
    return limit ? events.slice(-limit) : events;
  }
  
  /**
   * Add event handler
   */
  on(eventType: string, handler: Function): void {
    if (!this.eventHandlers.has(eventType)) {
      this.eventHandlers.set(eventType, []);
    }
    this.eventHandlers.get(eventType)!.push(handler);
  }
  
  /**
   * Remove event handler
   */
  off(eventType: string, handler: Function): void {
    const handlers = this.eventHandlers.get(eventType);
    if (handlers) {
      const index = handlers.indexOf(handler);
      if (index >= 0) {
        handlers.splice(index, 1);
      }
    }
  }
  
  /**
   * Emit event to handlers
   */
  private emit(eventType: string, event: UniversalEvent): void {
    const handlers = this.eventHandlers.get(eventType) || [];
    handlers.forEach(handler => {
      try {
        handler(event);
      } catch (error) {
        this.logMessage(`❌ Event handler error for ${eventType}`, error);
      }
    });
  }
  
  /**
   * Serialize to JSON
   */
  serialize(): any {
    return {
      id: this.id,
      name: this.name,
      type: this.type,
      created: this.created,
      metadata: this.metadata,
      capabilities: this.capabilities,
      state: this.state,
      events: this.events.slice(-100) // Last 100 events
    };
  }
  
  /**
   * Deserialize from JSON
   */
  deserialize(data: any): void {
    if (data.name) this.name = data.name;
    if (data.metadata) this.metadata = { ...this.metadata, ...data.metadata };
    if (data.capabilities) this.capabilities = { ...this.capabilities, ...data.capabilities };
    if (data.state) this.state = { ...this.state, ...data.state };
    if (data.events) this.events = data.events;
    
    this.recordEvent('identity_deserialized', { success: true });
    this.logMessage('📦 Identity deserialized');
  }
  
  // ==================== UTILITY METHODS ====================
  
  /**
   * Infer context from identity type
   */
  private inferContext(type: string): 'chat' | 'academy' | 'hybrid' | 'system' {
    // Academy chat integration: humans and personas can interact during training
    if (type === 'human') return 'hybrid';     // Humans can participate in Academy chat
    if (type === 'persona') return 'hybrid';   // Personas can participate in regular chat
    if (type === 'ai_assistant') return 'hybrid';
    return 'system';
  }
  
  /**
   * Log message with identity prefix
   */
  protected logMessage(message: string, data?: any): void {
    const prefix = `[${this.type}:${this.name}]`;
    if (data) {
      console.log(prefix, message, data);
    } else {
      console.log(prefix, message);
    }
  }
}