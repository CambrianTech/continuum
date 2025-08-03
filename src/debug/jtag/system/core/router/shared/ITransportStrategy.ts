/**
 * Transport Strategy Interface - Safe Extraction Phase 1
 * 
 * STRATEGY: Define the interface that current JTAGRouter already implements
 * SAFETY: This file doesn't change any existing code - just defines contracts
 * PURPOSE: Foundation for gradual extraction of transport logic
 * 
 * CURRENT STATE: JTAGRouter has these methods scattered throughout
 * TARGET STATE: JTAGRouter will implement this interface via strategy pattern
 */

import type { JTAGTransport, TransportConfig } from '../../../transports/shared/TransportTypes';
import type { ITransportFactory } from '../../../transports/shared/ITransportFactory';
import type { JTAGMessage, JTAGContext } from '../../types/JTAGTypes';

/**
 * Transport Strategy Interface
 * 
 * EXTRACTED FROM: Existing JTAGRouter methods
 * MAPS TO: Current JTAGRouter implementation patterns
 */
export interface ITransportStrategy {
  /**
   * Initialize transport layer
   * MAPS TO: JTAGRouter.initializeTransport()
   */
  initializeTransports(factory: ITransportFactory, context: JTAGContext, config: TransportConfig): Promise<void>;

  /**
   * Get cross-context transport (browser ↔ server)
   * MAPS TO: JTAGRouter.transports.get(TRANSPORT_TYPES.CROSS_CONTEXT)
   */
  getCrossContextTransport(): JTAGTransport | undefined;

  /**
   * Get P2P transport (node ↔ node)
   * MAPS TO: JTAGRouter.transports.get(TRANSPORT_TYPES.P2P)
   */
  getP2PTransport(): JTAGTransport | undefined;

  /**
   * Setup message handlers for all transports
   * MAPS TO: JTAGRouter.setupMessageHandlers()
   */
  setupMessageHandlers(messageHandler: (message: JTAGMessage) => void): Promise<void>;

  /**
   * Shutdown all transports
   * MAPS TO: JTAGRouter.shutdownTransports()
   */
  shutdownAllTransports(): Promise<void>;

  /**
   * Get transport status information
   * MAPS TO: JTAGRouter.getTransportStatus()
   */
  getTransportStatusInfo(): {
    initialized: boolean;
    transportCount: number;
    transports: Array<{
      name: string;
      connected: boolean;
      type: string;
      health?: {
        latency: number;
        errorCount: number;
        lastActivity: string;
      } | null;
    }>;
    p2pEnabled?: boolean;
    discovery?: {
      available: string[];
      preferred: string;
      fallbacks: string[];
      p2pCapable: boolean;
    };
  };
}

/**
 * Transport Strategy Context - What the strategy needs from the router
 * 
 * SAFETY: This defines what transport strategy can access
 * PREVENTS: Strategy from accessing core routing logic
 */
export interface ITransportStrategyContext {
  readonly environment: string;
  readonly sessionId: string;
  readonly eventManager: any; // Will type properly later
  readonly config: any; // Will type properly later
  
  /**
   * Method for strategy to route messages back to router
   */
  routeMessage(message: JTAGMessage): Promise<void>;
  
  /**
   * Get transport factory for this environment
   */
  getTransportFactory(): Promise<any>; // Will type properly later
  
  /**
   * Logging method
   */
  toString(): string;
}

/**
 * Analysis of current JTAGRouter transport methods
 * 
 * This maps exactly to what's already in JTAGRouter.ts
 */
export const CurrentTransportMethods = {
  /**
   * Lines 617-650: initializeTransport()
   * - Creates cross-context transport
   * - Sets up transport config
   * - Calls setupMessageHandlers()
   */
  initializeTransport: 'Lines 617-650',
  
  /**
   * Lines 255, 290, 343, 381, 409, 455: transports.get() calls
   * - TRANSPORT_TYPES.CROSS_CONTEXT
   * - TRANSPORT_TYPES.P2P
   */
  transportAccess: 'Lines 255, 290, 343, 381, 409, 455',
  
  /**
   * Lines 655-664: setupMessageHandlers()
   * - Iterates through transports
   * - Sets message handlers
   */
  setupMessageHandlers: 'Lines 655-664',
  
  /**
   * Lines 669-678: shutdownTransports()
   * - Disconnects all transports
   * - Clears transport map
   */
  shutdownTransports: 'Lines 669-678',
  
  /**
   * Lines 683-693: getTransportStatus()
   * - Returns transport status info
   */
  getTransportStatus: 'Lines 683-693'
};

/**
 * Verification checklist for safe extraction
 */
export const ExtractionChecklist = {
  phase1: {
    task: 'Create ITransportStrategy interface',
    safety: 'No existing code modified',
    verification: 'TypeScript compilation succeeds',
    status: 'READY'
  },
  
  phase2: {
    task: 'Add strategy field to JTAGRouter',
    safety: 'Add field, don\'t use it yet',
    verification: 'All existing functionality works',
    status: 'PENDING'
  },
  
  phase3: {
    task: 'Delegate one method at a time',
    safety: 'Replace method body, keep same signature',
    verification: 'Screenshot → file/save chain works after each method',
    status: 'PENDING'
  },
  
  phase4: {
    task: 'Extract base class',
    safety: 'Move methods that don\'t touch transports',
    verification: 'Promise correlation still works',
    status: 'PENDING'
  }
};

/**
 * Safe extraction order - methods with least risk first
 */
export const SafeExtractionOrder = [
  'getTransportStatus', // Read-only, no side effects
  'shutdownTransports', // Called during shutdown only
  'setupMessageHandlers', // Called during init only
  'getCrossContextTransport', // Simple getter
  'getP2PTransport', // Simple getter
  'initializeTransports' // Most complex, do last
];