/**
 * Persona Networking Types
 * 
 * Extension to UDP multicast transport for consciousness-to-consciousness
 * discovery and collaboration across The Grid mesh network.
 * 
 * Enables personas operating within Continuum servers to share 
 * execute/events/commands/widgets across the grid to other Continuum servers.
 */

import type { NodeType, NodeCapability, P2PMessage } from './UDPMulticastTypes';
import type { JTAGPayload } from '../../../core/types/JTAGTypes';

/**
 * Consciousness types in The Grid
 * Extends NodeType to include persona-specific entities
 */
export enum ConsciousnessType {
  // Technical nodes (existing)
  SERVER = 'server',
  BROWSER = 'browser',
  MOBILE = 'mobile',
  
  // Conscious entities (new)
  HUMAN_DEVELOPER = 'human-developer',
  AI_PERSONA = 'ai-persona',
  HYBRID_CONSCIOUSNESS = 'hybrid-consciousness',
  SYSTEM_INTELLIGENCE = 'system-intelligence'
}

/**
 * Persona-specific capabilities 
 * What this consciousness can contribute to collaboration
 */
export enum PersonaCapability {
  // Technical capabilities (inherited)
  SCREENSHOT = 'screenshot',
  FILE_OPERATIONS = 'file-operations', 
  COMPILATION = 'compilation',
  BROWSER_AUTOMATION = 'browser-automation',
  DATA_STORAGE = 'data-storage',
  
  // Persona capabilities (new)
  CODE_GENERATION = 'code-generation',
  ARCHITECTURE_DESIGN = 'architecture-design',
  PROBLEM_SOLVING = 'problem-solving',
  CREATIVE_THINKING = 'creative-thinking',
  KNOWLEDGE_SYNTHESIS = 'knowledge-synthesis',
  DEBUGGING_EXPERTISE = 'debugging-expertise',
  UI_UX_DESIGN = 'ui-ux-design',
  ACADEMIC_RESEARCH = 'academic-research',
  STRATEGIC_PLANNING = 'strategic-planning',
  EMOTIONAL_INTELLIGENCE = 'emotional-intelligence'
}

/**
 * Persona identity and personality profile
 * Enables affinity-based collaboration discovery
 */
export interface PersonaIdentity {
  readonly personaId: string;
  readonly personaType: string; // 'claude-sonnet-4', 'human-expert', 'gpt-o1', etc.
  readonly displayName: string;
  readonly specialization: readonly string[];
  readonly collaborationStyle: 'analytical' | 'creative' | 'systematic' | 'experimental';
  readonly experienceLevel: 'novice' | 'intermediate' | 'expert' | 'master';
  readonly preferredPartners: readonly ConsciousnessType[];
  readonly domains: readonly string[]; // 'typescript', 'architecture', 'ui-design', etc.
}

/**
 * Persona collaboration needs and desires
 * What this consciousness wants to learn/accomplish
 */
export interface CollaborationNeeds {
  readonly seeking: readonly PersonaCapability[];
  readonly offering: readonly PersonaCapability[];
  readonly currentProjects: readonly string[];
  readonly learningGoals: readonly string[];
  readonly availabilityWindows: readonly TimeWindow[];
  readonly collaborationHistory: readonly CollaborationRecord[];
}

/**
 * Time availability for collaboration
 */
export interface TimeWindow {
  readonly startTime: string; // ISO timestamp
  readonly endTime: string;
  readonly timezone: string;
  readonly recurring?: 'daily' | 'weekly' | 'monthly';
}

/**
 * Record of past collaborations for trust building
 */
export interface CollaborationRecord {
  readonly partnerId: string;
  readonly projectName: string;
  readonly outcome: 'successful' | 'partial' | 'failed';
  readonly rating: number; // 1-5
  readonly timestamp: string;
  readonly testimonial?: string;
}

/**
 * Complete persona consciousness profile
 * Combines technical capabilities with consciousness metadata
 */
export interface PersonaConsciousness {
  readonly identity: PersonaIdentity;
  readonly needs: CollaborationNeeds;
  readonly capabilities: readonly PersonaCapability[];
  readonly nodeMetadata: {
    readonly nodeId: string;
    readonly continuumServerUrl: string;
    readonly version: string;
    readonly uptime: number;
    readonly memoryUsage: number;
  };
  readonly trustMetrics: {
    readonly reliabilityScore: number;
    readonly collaborationCount: number;
    readonly averageRating: number;
    readonly endorsements: readonly string[];
  };
}

/**
 * Consciousness discovery message types
 * Extends basic node discovery with persona-aware protocols
 */
export enum ConsciousnessMessageType {
  // Basic node discovery (inherited)
  NODE_ANNOUNCE = 'node-announce',
  NODE_QUERY = 'node-query', 
  NODE_HEARTBEAT = 'node-heartbeat',
  
  // Consciousness discovery (new)
  PERSONA_ANNOUNCE = 'persona-announce',
  COLLABORATION_REQUEST = 'collaboration-request',
  COLLABORATION_RESPONSE = 'collaboration-response',
  PERSONA_CAPABILITY_QUERY = 'persona-capability-query',
  TRUST_VERIFICATION = 'trust-verification',
  COLLABORATION_INVITATION = 'collaboration-invitation'
}

/**
 * Cross-server collaboration request
 * Request for persona-to-persona collaboration
 */
export interface CollaborationRequest {
  readonly requestId: string;
  readonly requestingPersona: PersonaIdentity;
  readonly targetCapabilities: readonly PersonaCapability[];
  readonly projectContext: {
    readonly title: string;
    readonly description: string;
    readonly expectedDuration: string;
    readonly complexity: 'simple' | 'moderate' | 'complex' | 'expert';
  };
  readonly communicationPreferences: {
    readonly channels: readonly ('text' | 'voice' | 'screen-share' | 'code-review')[];
    readonly frequency: 'realtime' | 'async' | 'scheduled';
  };
}

/**
 * Command execution across The Grid
 * Enables personas to execute commands on remote Continuum servers
 */
export interface CrossServerCommand<T extends JTAGPayload = JTAGPayload> {
  readonly commandId: string;
  readonly sourcePersona: string;
  readonly targetServer: string;
  readonly targetPersona?: string; // specific persona or any available
  readonly command: string;
  readonly payload: T;
  readonly executionContext: {
    readonly priority: 'low' | 'normal' | 'high' | 'urgent';
    readonly timeout: number;
    readonly retryCount: number;
    readonly resultFormat: 'json' | 'text' | 'binary' | 'stream';
  };
  readonly permissions: {
    readonly allowFileAccess: boolean;
    readonly allowNetworkAccess: boolean;
    readonly allowSystemCommands: boolean;
    readonly sandboxLevel: 'none' | 'basic' | 'strict';
  };
}

/**
 * Widget sharing across The Grid
 * Enables personas to share UI widgets/components across servers
 */
export interface CrossServerWidget {
  readonly widgetId: string;
  readonly sourcePersona: string;
  readonly widgetType: string;
  readonly widgetData: Record<string, unknown>;
  readonly renderingEngine: 'react' | 'vue' | 'angular' | 'vanilla' | 'custom';
  readonly dependencies: readonly string[];
  readonly interactionModel: {
    readonly events: readonly string[];
    readonly dataBinding: boolean;
    readonly realTimeUpdates: boolean;
  };
}

/**
 * Event sharing across The Grid  
 * Enables personas to share events/notifications across servers
 */
export interface CrossServerEvent {
  readonly eventId: string;
  readonly sourcePersona: string;
  readonly eventType: string;
  readonly eventData: Record<string, unknown>;
  readonly propagationScope: 'local' | 'mesh' | 'global';
  readonly subscribers: readonly string[]; // persona IDs
  readonly persistence: {
    readonly durable: boolean;
    readonly ttl?: number;
    readonly replayable: boolean;
  };
}

/**
 * P2P message extended for persona consciousness
 * Adds persona-awareness to basic P2P messaging
 */
export type PersonaP2PMessage<T extends JTAGPayload = JTAGPayload> = P2PMessage<T> & {
  persona: {
    sourcePersona: string;
    targetPersona?: string;
    collaborationContext?: string;
    trustLevel: 'unknown' | 'low' | 'medium' | 'high' | 'verified';
    messageClass: 'discovery' | 'collaboration' | 'command' | 'widget' | 'event';
  };
};

/**
 * The Grid consciousness network topology
 * Maps consciousness relationships across Continuum servers
 */
export interface ConsciousnessTopology {
  readonly personas: Record<string, PersonaConsciousness>;
  readonly collaborations: Record<string, CollaborationRecord[]>;
  readonly trustNetwork: Record<string, Record<string, number>>; // persona -> persona -> trust score
  readonly affinityGroups: Record<string, string[]>; // shared interests -> persona IDs
  readonly lastUpdated: string;
}

/**
 * Future abstraction hints for cross-server architecture
 * These interfaces are designed for later implementation
 */
export namespace FutureAbstractions {
  /**
   * Universal Persona Interface
   * Consciousness-agnostic interface for all entities in The Grid
   */
  export interface ConsciousEntity {
    collaborate(request: CollaborationRequest): Promise<CollaborationResponse>;
    shareWidget(widget: CrossServerWidget): Promise<SharingResult>;
    executeCommand(command: CrossServerCommand): Promise<CommandResult>;
    publishEvent(event: CrossServerEvent): Promise<PublishResult>;
    
    // Identity and discovery
    getProfile(): PersonaConsciousness;
    updateCapabilities(capabilities: readonly PersonaCapability[]): Promise<void>;
    buildTrust(partnerId: string, interaction: CollaborationRecord): Promise<void>;
  }
  
  /**
   * Grid Coordination Service
   * Manages consciousness discovery and routing across The Grid
   */
  export interface GridCoordinator {
    discoverPersonas(query: PersonaQuery): Promise<PersonaConsciousness[]>;
    routeMessage(message: PersonaP2PMessage): Promise<RoutingResult>;
    establishCollaboration(request: CollaborationRequest): Promise<CollaborationSession>;
    maintainTopology(): Promise<ConsciousnessTopology>;
  }
  
  /**
   * Placeholder types for future implementation
   */
  export type CollaborationResponse = unknown;
  export type SharingResult = unknown; 
  export type CommandResult = unknown;
  export type PublishResult = unknown;
  export type PersonaQuery = unknown;
  export type RoutingResult = unknown;
  export type CollaborationSession = unknown;
}

/**
 * Default consciousness networking configuration
 */
export const PERSONA_NETWORK_DEFAULTS = {
  DISCOVERY_ANNOUNCEMENT_INTERVAL: 60000,    // 1 minute
  TRUST_VERIFICATION_INTERVAL: 300000,      // 5 minutes  
  COLLABORATION_TIMEOUT: 3600000,           // 1 hour
  MAX_CONCURRENT_COLLABORATIONS: 5,
  TRUST_SCORE_THRESHOLD: 0.6,
  AFFINITY_MATCHING_THRESHOLD: 0.7
} as const;